//! Obtaining an IAM Identity Center session from inside the application
//! (`openspec` change `xonho-0011`, `sso-sign-in` spec).
//!
//! The device authorization flow is three calls and a wait: register a client,
//! start an authorization the user completes in a browser, then ask for the
//! token until they have. The three calls are this port; the wait is
//! [`crate::sso`]'s own loop, which is where every interesting decision lives —
//! how long to wait, when to stop, and which of four failures happened.
//!
//! The port exists so that loop can be tested. Against the real provider each
//! branch needs a human doing something unusual at the right moment; against a
//! double each branch is a test. This is the same reason
//! [`crate::store::ObjectStore`] exists, and the boundary is drawn the same
//! way: no `aws_sdk_ssooidc` type appears in any signature here, so the
//! adapter can be replaced without the flow noticing.

use std::fmt;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::error::{Error, Result};

/// A secret that appears in the sign-in flow.
///
/// Access tokens, refresh tokens, client secrets and device codes are all
/// bearer material: whoever holds one can act. They are carried in this type
/// rather than in `String` for the same reason
/// [`crate::credentials::CredentialSecret`] exists — the hand-written
/// [`fmt::Debug`] below is what stops a stray `{:?}` from being the one place
/// a token reaches the log.
#[derive(Clone, PartialEq, Eq)]
pub struct SignInSecret(String);

impl SignInSecret {
    /// Wrap a secret received from the provider.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// The secret itself. Deliberately `pub(crate)`: it is needed to sign the
    /// next call and to write the token cache, and nowhere else.
    pub(crate) fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SignInSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SignInSecret(<redacted>)")
    }
}

/// Where a connection signs in, read from the profile's `sso_session`.
///
/// All four fields are required by the provider, and none of them can be
/// guessed: a profile that does not declare them is reported as not saying
/// where to sign in, which is a configuration cause rather than a failed
/// attempt (`sso-sign-in`, "A session can be obtained from within the
/// application").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignInLocation {
    /// The `sso_session` name. Also the identity the token cache is keyed on —
    /// see [`crate::sso`]'s cache writer, where using the start URL instead
    /// produces a file nothing reads.
    pub session_name: String,
    /// The Identity Center portal URL.
    pub start_url: String,
    /// The region the Identity Center instance lives in, which is not
    /// necessarily the region any bucket lives in.
    pub region: String,
    /// The scopes the profile declares, in the order it declares them. Empty
    /// means the profile declared none, and none are sent.
    pub scopes: Vec<String>,
}

/// A client registration, as the provider issued it.
///
/// Kept beyond the sign-in it was made for: stored beside the token, it is
/// what lets the SDK refresh that token later without another trip through a
/// browser (`design.md`, "The entry is written refreshable, or not at all").
#[derive(Debug, Clone)]
pub struct ClientRegistration {
    /// Public material: it identifies the client, it does not authenticate it.
    pub client_id: String,
    /// The half that does authenticate it.
    pub client_secret: SignInSecret,
    /// When the registration itself stops being usable — a longer life than
    /// any token it obtains, and the real interval between browser trips.
    pub registration_expires_at: SystemTime,
}

/// An authorization the user has been asked to complete.
#[derive(Debug, Clone)]
pub struct DeviceAuthorization {
    /// Proof, when polling, that this attempt is the one that was started.
    /// Secret: it is what the token is issued against.
    pub device_code: SignInSecret,
    /// The short code the user reads off the screen and types into the
    /// browser. Shown, not hidden — it is useless without the session it
    /// belongs to, and hiding it would defeat the one case this flow exists
    /// for: a browser that did not open.
    pub user_code: String,
    /// Where the user completes the authorization.
    pub verification_uri: String,
    /// The same, with the code already filled in. What gets opened.
    pub verification_uri_complete: String,
    /// When this attempt stops being completable. Polling past it is asking
    /// for an error the provider has already told us to expect.
    pub expires_at: SystemTime,
    /// The minimum wait the provider asked for between polls.
    pub interval: Duration,
}

/// A session obtained from the provider.
#[derive(Debug, Clone)]
pub struct SsoToken {
    /// What authenticates calls until it expires.
    pub access_token: SignInSecret,
    /// Present when the provider issued one. Its absence is why an entry can
    /// be written unrefreshable, and why that case is worth reporting rather
    /// than shrugging at.
    pub refresh_token: Option<SignInSecret>,
    /// When the access token stops working.
    pub expires_at: SystemTime,
}

/// What one poll for a token answered.
///
/// Three variants and not one: waiting and being told to wait longer are both
/// ordinary progress, not failures, and folding either into an error would
/// make the loop unable to tell "keep going" from "stop". The outcomes that
/// do end an attempt — declined, expired, unreachable — arrive as
/// [`crate::error::Error`] instead, because each of them is a cause a user
/// has to be told apart from the others.
#[derive(Debug)]
pub enum TokenAnswer {
    /// The user finished. This is the session.
    Issued(SsoToken),
    /// The user has not finished yet. Wait and ask again.
    Pending,
    /// We asked too often. Wait longer than before, then ask again.
    SlowDown,
}

/// The device authorization flow, behind one object-safe async trait.
///
/// Three calls, in the order they must happen. Each takes the location
/// explicitly rather than being constructed against one, so a single
/// implementation serves every connection in the application.
#[async_trait::async_trait]
pub trait SsoSignIn: Send + Sync {
    /// Register this application with the provider.
    ///
    /// Issues the client identity the rest of the flow is conducted under. The
    /// result outlives the sign-in: stored with the token, it is what makes
    /// that token refreshable.
    async fn register_client(&self, at: &SignInLocation) -> Result<ClientRegistration>;

    /// Begin an authorization for the user to complete.
    ///
    /// Returns the code to show, the address to open, how long the attempt
    /// lives and how often it may be polled. Every one of those is the
    /// provider's to decide; none is ours to assume.
    async fn start_device_authorization(
        &self,
        at: &SignInLocation,
        client: &ClientRegistration,
    ) -> Result<DeviceAuthorization>;

    /// Ask whether the user has finished.
    ///
    /// Answers [`TokenAnswer`] while the attempt is alive, and fails with the
    /// cause when it is not: a declined authorization, an expired attempt, and
    /// an unreachable provider are three different things to be told, and the
    /// classifier keeps them apart rather than flattening them into
    /// "sign-in failed".
    async fn create_token(
        &self,
        at: &SignInLocation,
        client: &ClientRegistration,
        authorization: &DeviceAuthorization,
    ) -> Result<TokenAnswer>;
}

/// What the caller waits on, injected so a test never sleeps.
///
/// The loop below is almost entirely about time — how long to wait, whether
/// the attempt is still alive — so time is a parameter rather than something
/// read from the machine. Every case in the spec is then a test that runs in
/// microseconds instead of a test nobody runs.
#[async_trait::async_trait]
pub trait Waiter: Send + Sync {
    /// The current time.
    fn now(&self) -> SystemTime;
    /// Wait, at least, this long.
    async fn wait(&self, how_long: Duration);
}

/// A shared "stop" the user can set.
///
/// Cloned rather than borrowed: the window keeps one and the flow keeps
/// another, and abandoning has to work from a thread that is not the one
/// polling.
#[derive(Debug, Clone, Default)]
pub struct Abandon(std::sync::Arc<std::sync::atomic::AtomicBool>);

impl Abandon {
    /// Ask the flow to stop. Idempotent.
    pub fn now(&self) {
        self.0.store(true, std::sync::atomic::Ordering::SeqCst);
    }

    /// Whether stopping has been asked for.
    pub fn asked(&self) -> bool {
        self.0.load(std::sync::atomic::Ordering::SeqCst)
    }
}

/// A session, and the registration that will let it be refreshed.
///
/// The two travel together because they are written together: an entry
/// without the registration is an entry the SDK will not refresh, which costs
/// a browser trip per token expiry instead of per registration expiry.
#[derive(Debug)]
pub struct ObtainedSession {
    /// The session itself.
    pub token: SsoToken,
    /// The registration it was obtained under.
    pub registration: ClientRegistration,
}

/// How a sign-in ended, when it did not fail.
#[derive(Debug)]
pub enum SignInOutcome {
    /// The user completed it.
    Session(ObtainedSession),
    /// The user stopped it. Not a failure: nobody needs to be told the cause
    /// of something they did on purpose.
    Abandoned,
}

/// How much longer to wait each time the provider says we are asking too
/// often. RFC 8628 §3.5 names five seconds, and the provider does not send a
/// new interval with the answer, so this is the client's to decide.
const SLOW_DOWN_STEP: Duration = Duration::from_secs(5);

/// Run the device authorization flow to its end.
///
/// `show` is handed the authorization as soon as it exists, before any
/// waiting: the user code has to be on screen while the user is being waited
/// for, not after. Everything else is this function's business — the interval
/// the provider asked for, widening it when told to, stopping at the
/// attempt's own expiry, and noticing that the user gave up.
pub async fn sign_in(
    port: &dyn SsoSignIn,
    waiter: &dyn Waiter,
    at: &SignInLocation,
    abandon: &Abandon,
    show: impl FnOnce(&DeviceAuthorization),
) -> Result<SignInOutcome> {
    let registration = port.register_client(at).await?;
    let authorization = port.start_device_authorization(at, &registration).await?;
    show(&authorization);

    let mut interval = authorization.interval;
    loop {
        if abandon.asked() {
            return Ok(SignInOutcome::Abandoned);
        }
        // Checked before waiting as well as after: an attempt whose window
        // has already closed should not cost the user another interval of
        // watching a spinner that cannot succeed.
        if waiter.now() >= authorization.expires_at {
            return Err(crate::error::Error::SignIn {
                sso_session: at.session_name.clone(),
                problem: crate::error::SignInProblem::Expired,
            });
        }

        waiter.wait(interval).await;

        if abandon.asked() {
            return Ok(SignInOutcome::Abandoned);
        }
        // And again after waiting, which is the check that matters: a wait
        // can land past the window, and asking the provider for a token it
        // has already stopped issuing spends a request to be told something
        // we could read off the clock (`sso-sign-in`: never polls after the
        // attempt's own expiry).
        if waiter.now() >= authorization.expires_at {
            return Err(crate::error::Error::SignIn {
                sso_session: at.session_name.clone(),
                problem: crate::error::SignInProblem::Expired,
            });
        }

        match port.create_token(at, &registration, &authorization).await? {
            TokenAnswer::Issued(token) => {
                return Ok(SignInOutcome::Session(ObtainedSession {
                    token,
                    registration,
                }));
            }
            TokenAnswer::Pending => {}
            TokenAnswer::SlowDown => interval += SLOW_DOWN_STEP,
        }
    }
}

/// The waiter the application actually runs on.
///
/// Separate from the loop so the loop can be tested without spending the time
/// it waits. This one spends it: `tokio::time::sleep` on the runtime the rest
/// of the AWS work already uses.
#[derive(Debug, Clone, Copy, Default)]
pub struct RealTime;

#[async_trait::async_trait]
impl Waiter for RealTime {
    fn now(&self) -> SystemTime {
        SystemTime::now()
    }

    async fn wait(&self, how_long: Duration) {
        tokio::time::sleep(how_long).await;
    }
}

/// The home directory the AWS tooling reads its configuration from.
///
/// Mirrors `aws_runtime::fs_util::home_dir` deliberately, including the
/// Windows fallbacks: this is the one place where being *nearly* right means
/// writing a valid file into a directory nothing reads, which looks exactly
/// like signing in and having nothing happen. `HOME` first on every platform,
/// because the SDK checks it first on every platform.
pub fn home_dir() -> Option<PathBuf> {
    if let Some(home) = std::env::var_os("HOME").filter(|home| !home.is_empty()) {
        return Some(PathBuf::from(home));
    }
    if cfg!(windows) {
        if let Some(profile) = std::env::var_os("USERPROFILE").filter(|p| !p.is_empty()) {
            return Some(PathBuf::from(profile));
        }
        if let (Some(drive), Some(path)) = (
            std::env::var_os("HOMEDRIVE").filter(|d| !d.is_empty()),
            std::env::var_os("HOMEPATH").filter(|p| !p.is_empty()),
        ) {
            let mut drive = PathBuf::from(drive).into_os_string();
            drive.push(path);
            return Some(PathBuf::from(drive));
        }
    }
    None
}

/// The file a session for `session_name` is cached in.
///
/// **The `sso_session` name, never the start URL.** `aws-config` keys this
/// cache on the start URL for its *credentials* provider and on the session
/// name for its *token* provider (`aws-config-1.10.1/src/sso/cache.rs`,
/// `cached_token_path`), and the token provider is the one that reads what we
/// write. Hashing the wrong string produces a file nothing ever opens.
pub fn cache_path(home: &Path, session_name: &str) -> PathBuf {
    let digest = hex::encode(<sha1::Sha1 as sha1::Digest>::digest(
        session_name.as_bytes(),
    ));
    home.join(".aws")
        .join("sso")
        .join("cache")
        .join(format!("{digest}.json"))
}

/// Write an obtained session where credential resolution already looks.
///
/// The JSON mirrors `aws-config`'s own `save_cached_token`, key for key and in
/// its order, using the same JSON writer and the same timestamp rendering. It
/// is mirrored rather than called because that function is `pub(super)` — see
/// `design.md`. The round-trip test is what keeps the mirror true.
///
/// The registration travels with the token on purpose: `CachedSsoToken`
/// reports itself refreshable only when the client id, client secret, refresh
/// token and registration expiry are all present, and an unrefreshable entry
/// costs the user a browser trip every time an eight-hour token lapses.
pub fn write_session(
    home: &Path,
    at: &SignInLocation,
    obtained: &ObtainedSession,
) -> Result<PathBuf> {
    use aws_smithy_types::DateTime;
    use aws_smithy_types::date_time::Format;

    let path = cache_path(home, &at.session_name);
    let directory = path.parent().expect("the cache path always has a parent");

    let rendered = |time: SystemTime| -> Result<String> {
        DateTime::from(time)
            .fmt(Format::DateTime)
            .map_err(|error| Error::TokenCacheNotWritable {
                path: path.display().to_string(),
                detail: format!("a timestamp could not be rendered: {error}"),
            })
    };
    let expires_at = rendered(obtained.token.expires_at)?;
    let registration_expires_at = rendered(obtained.registration.registration_expires_at)?;

    let mut body = String::new();
    let mut writer = aws_smithy_json::serialize::JsonObjectWriter::new(&mut body);
    writer
        .key("accessToken")
        .string(obtained.token.access_token.expose());
    writer.key("expiresAt").string(&expires_at);
    if let Some(refresh_token) = &obtained.token.refresh_token {
        writer.key("refreshToken").string(refresh_token.expose());
    }
    writer
        .key("clientId")
        .string(&obtained.registration.client_id);
    writer
        .key("clientSecret")
        .string(obtained.registration.client_secret.expose());
    writer
        .key("registrationExpiresAt")
        .string(&registration_expires_at);
    writer.key("region").string(&at.region);
    writer.key("startUrl").string(&at.start_url);
    writer.finish();

    let failed = |what: &str, error: std::io::Error| Error::TokenCacheNotWritable {
        path: path.display().to_string(),
        detail: format!("{what}: {error}"),
    };

    std::fs::create_dir_all(directory).map_err(|error| failed("creating the directory", error))?;

    // Written beside the target and renamed over it. This directory belongs
    // to the AWS CLI as much as to us: a half-written file here is not our
    // bug to suffer, it is another tool's bug that we caused.
    let temporary = directory.join(format!(".{}.caixonho", uniquish(&path)));
    write_private(&temporary, &body).map_err(|error| {
        let _ = std::fs::remove_file(&temporary);
        failed("writing the session", error)
    })?;
    std::fs::rename(&temporary, &path).map_err(|error| {
        let _ = std::fs::remove_file(&temporary);
        failed("saving the session", error)
    })?;

    Ok(path)
}

/// A name for the temporary file that will not collide with a concurrent
/// sign-in. The target's own digest plus this process's id is enough: two
/// sign-ins to the same session from the same process are already serialized
/// by the window that starts them.
fn uniquish(target: &Path) -> String {
    let stem = target
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("session");
    format!("{stem}.{}", std::process::id())
}

/// Write a file only its owner can read, where the platform has such a notion.
///
/// The AWS CLI writes this file `0600`; a token readable by every account on
/// a shared machine would be a strange thing to leave behind, whatever the
/// original does.
fn write_private(path: &Path, body: &str) -> std::io::Result<()> {
    use std::io::Write as _;

    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(body.as_bytes())?;
    file.sync_all()
}

#[cfg(test)]
pub(crate) mod double {
    //! Hand-rolled test double: one constructor per canned behaviour, so a
    //! test names the scenario it simulates instead of assembling state.
    //!
    //! Every branch of the flow is reachable from here, which is the whole
    //! argument for the port. Against the live provider, "the user declined"
    //! and "the attempt expired" need a person doing something unusual at a
    //! particular second; here they are two constructors.

    use std::sync::Mutex;
    use std::time::{Duration, SystemTime};

    use super::{
        ClientRegistration, DeviceAuthorization, SignInLocation, SignInSecret, SsoSignIn, SsoToken,
        TokenAnswer,
    };
    use crate::error::{Error, Result, SignInProblem};

    /// What one poll answers.
    enum Step {
        Pending,
        SlowDown,
        Issue,
        Fail(fn() -> Error),
    }

    /// A canned [`SsoSignIn`] for tests.
    pub(crate) struct SsoSignInDouble {
        /// Answers, in order. The last one repeats if polling outlives it,
        /// so a test never has to guess how many polls the loop will make.
        script: Mutex<Vec<Step>>,
        polls: Mutex<usize>,
        interval: Duration,
        expires_in: Duration,
        /// Set when registering is what should fail.
        registration_failure: Option<fn() -> Error>,
        /// Set when starting the authorization is what should fail.
        start_failure: Option<fn() -> Error>,
    }

    impl SsoSignInDouble {
        fn with(script: Vec<Step>) -> Self {
            Self {
                script: Mutex::new(script),
                polls: Mutex::new(0),
                interval: Duration::from_secs(5),
                expires_in: Duration::from_secs(600),
                registration_failure: None,
                start_failure: None,
            }
        }

        /// Answers `Pending` `n` times, then issues a token.
        pub(crate) fn pending_then_issues(n: usize) -> Self {
            let mut script: Vec<Step> = (0..n).map(|_| Step::Pending).collect();
            script.push(Step::Issue);
            Self::with(script)
        }

        /// Answers `SlowDown` once, then issues a token.
        pub(crate) fn slows_down_then_issues() -> Self {
            Self::with(vec![Step::SlowDown, Step::Issue])
        }

        /// The user declined in the browser.
        pub(crate) fn declined() -> Self {
            Self::with(vec![Step::Fail(|| Error::SignIn {
                sso_session: "double".into(),
                problem: SignInProblem::Declined,
            })])
        }

        /// The attempt's window closed before the user finished.
        pub(crate) fn attempt_expired() -> Self {
            Self::with(vec![Step::Fail(|| Error::SignIn {
                sso_session: "double".into(),
                problem: SignInProblem::Expired,
            })])
        }

        /// The provider could not be reached.
        pub(crate) fn unreachable() -> Self {
            Self::with(vec![Step::Fail(|| Error::Network {
                detail: "connect timed out".into(),
            })])
        }

        /// Registering the client fails before any authorization begins.
        pub(crate) fn cannot_register() -> Self {
            let mut this = Self::with(Vec::new());
            this.registration_failure = Some(|| Error::Network {
                detail: "connect timed out".into(),
            });
            this
        }

        /// The interval the provider asks for. Chained onto any constructor.
        pub(crate) fn interval(mut self, interval: Duration) -> Self {
            self.interval = interval;
            self
        }

        /// How long the attempt stays completable. Chained onto any
        /// constructor: a test that wants polling to outlive the attempt sets
        /// this shorter than the interval it expects to be waited.
        pub(crate) fn expires_in(mut self, expires_in: Duration) -> Self {
            self.expires_in = expires_in;
            self
        }

        /// How many times the token endpoint was asked.
        pub(crate) fn polls(&self) -> usize {
            *self.polls.lock().expect("double lock")
        }
    }

    #[async_trait::async_trait]
    impl SsoSignIn for SsoSignInDouble {
        async fn register_client(&self, _at: &SignInLocation) -> Result<ClientRegistration> {
            if let Some(fail) = self.registration_failure {
                return Err(fail());
            }
            Ok(ClientRegistration {
                client_id: "double-client".into(),
                client_secret: SignInSecret::new("double-client-secret"),
                registration_expires_at: SystemTime::UNIX_EPOCH + Duration::from_secs(90 * 86_400),
            })
        }

        async fn start_device_authorization(
            &self,
            _at: &SignInLocation,
            _client: &ClientRegistration,
        ) -> Result<DeviceAuthorization> {
            if let Some(fail) = self.start_failure {
                return Err(fail());
            }
            Ok(DeviceAuthorization {
                device_code: SignInSecret::new("double-device-code"),
                user_code: "WXYZ-1234".into(),
                verification_uri: "https://device.sso.example/".into(),
                verification_uri_complete: "https://device.sso.example/?user_code=WXYZ-1234".into(),
                expires_at: SystemTime::UNIX_EPOCH + self.expires_in,
                interval: self.interval,
            })
        }

        async fn create_token(
            &self,
            _at: &SignInLocation,
            _client: &ClientRegistration,
            _authorization: &DeviceAuthorization,
        ) -> Result<TokenAnswer> {
            let index = {
                let mut polls = self.polls.lock().expect("double lock");
                let index = *polls;
                *polls += 1;
                index
            };
            let script = self.script.lock().expect("double lock");
            // The last step repeats: a loop that polls once more than the
            // test scripted should not fall off the end of the script and
            // report something the test never asked for.
            let step = script
                .get(index)
                .or_else(|| script.last())
                .expect("script is never empty");
            match step {
                Step::Pending => Ok(TokenAnswer::Pending),
                Step::SlowDown => Ok(TokenAnswer::SlowDown),
                Step::Issue => Ok(TokenAnswer::Issued(SsoToken {
                    access_token: SignInSecret::new("double-access-token"),
                    refresh_token: Some(SignInSecret::new("double-refresh-token")),
                    expires_at: SystemTime::UNIX_EPOCH + Duration::from_secs(8 * 3_600),
                })),
                Step::Fail(fail) => Err(fail()),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;
    use std::time::{Duration, SystemTime};

    use super::double::SsoSignInDouble;
    use super::{Abandon, SignInLocation, SignInOutcome, Waiter, sign_in};
    use crate::error::{Error, SignInProblem};

    /// What a test runs after each wait, named so the struct field below
    /// stays readable.
    type DuringWait = Box<dyn Fn(&Recorded) + Send + Sync>;

    /// A clock that never sleeps: waiting advances a number and records what
    /// was asked for, so a test can assert the interval the provider named was
    /// honoured without spending it.
    struct Recorded {
        now: Mutex<SystemTime>,
        waits: Mutex<Vec<Duration>>,
        /// Run after each wait, so a test can abandon mid-flight without a
        /// second thread.
        during: Option<DuringWait>,
    }

    impl Recorded {
        fn new() -> Self {
            Self {
                now: Mutex::new(SystemTime::UNIX_EPOCH),
                waits: Mutex::new(Vec::new()),
                during: None,
            }
        }

        fn doing(mut self, during: impl Fn(&Recorded) + Send + Sync + 'static) -> Self {
            self.during = Some(Box::new(during));
            self
        }

        fn waits(&self) -> Vec<Duration> {
            self.waits.lock().expect("waits").clone()
        }
    }

    #[async_trait::async_trait]
    impl Waiter for Recorded {
        fn now(&self) -> SystemTime {
            *self.now.lock().expect("now")
        }

        async fn wait(&self, how_long: Duration) {
            {
                let mut now = self.now.lock().expect("now");
                *now += how_long;
            }
            self.waits.lock().expect("waits").push(how_long);
            if let Some(during) = &self.during {
                during(self);
            }
        }
    }

    fn somewhere() -> SignInLocation {
        SignInLocation {
            session_name: "corp".into(),
            start_url: "https://corp.awsapps.com/start".into(),
            region: "ap-southeast-1".into(),
            scopes: vec!["sso:account:access".into()],
        }
    }

    /// A session with everything the SDK needs to call it refreshable.
    fn obtained() -> super::ObtainedSession {
        super::ObtainedSession {
            token: super::SsoToken {
                access_token: super::SignInSecret::new("access-token-value"),
                refresh_token: Some(super::SignInSecret::new("refresh-token-value")),
                expires_at: SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000),
            },
            registration: super::ClientRegistration {
                client_id: "client-id-value".into(),
                client_secret: super::SignInSecret::new("client-secret-value"),
                registration_expires_at: SystemTime::UNIX_EPOCH
                    + Duration::from_secs(1_800_000_000),
            },
        }
    }

    #[test]
    fn the_cache_file_is_named_after_the_session_not_the_start_url() {
        // `aws-config-1.10.1/src/sso/cache.rs::cached_token_path` hashes the
        // `sso_session` name for the token provider and the start URL for the
        // credentials provider. The token provider reads what we write, so
        // the session name is the one that must be hashed. The expected value
        // below is SHA-1("corp") — pinned, because getting this wrong writes a
        // perfectly valid file that nothing will ever open.
        let path = super::cache_path(Path::new("/home/someone"), "corp");

        assert_eq!(
            path,
            Path::new("/home/someone/.aws/sso/cache")
                .join("ee0bfd2552fbd840c02cc48b6e823320543c450f.json"),
            "SHA-1 of the session name, hex, lowercase — verified against \
             `printf corp | shasum -a 1` rather than recalled"
        );
    }

    #[test]
    fn what_is_written_is_what_aws_config_reads_back() {
        // The compatibility surface, pinned. `aws-config`'s own writer is
        // `pub(super)` so it cannot be called; this asserts the mirror of it
        // key by key. Its reader (`json_parse_loop` in the same file) accepts
        // exactly these names, and `CachedSsoToken::refreshable()` requires
        // the four registration fields to be among them.
        //
        // **A stack bump is what breaks this test**, and that is the point:
        // ADR-0001 makes bumps a deliberate PR, and this is what that PR trips
        // over rather than a user discovering it at a login screen.
        let home = tempdir();
        let at = somewhere();

        let path = super::write_session(&home, &at, &obtained()).expect("the session is written");
        let body = std::fs::read_to_string(&path).expect("the file is there");

        for expected in [
            r#""accessToken":"access-token-value""#,
            r#""refreshToken":"refresh-token-value""#,
            r#""clientId":"client-id-value""#,
            r#""clientSecret":"client-secret-value""#,
            r#""region":"ap-southeast-1""#,
            r#""startUrl":"https://corp.awsapps.com/start""#,
            // Rendered by `aws_smithy_types::DateTime::fmt(Format::DateTime)`,
            // which is what `save_cached_token` uses.
            r#""expiresAt":"2023-11-14T22:13:20Z""#,
            r#""registrationExpiresAt":"2027-01-15T08:00:00Z""#,
        ] {
            assert!(body.contains(expected), "missing {expected} in {body}");
        }

        assert!(
            !body.contains("null"),
            "an absent field is omitted, never written as null: {body}"
        );
    }

    #[test]
    fn a_session_without_a_refresh_token_omits_the_key_rather_than_emptying_it() {
        let home = tempdir();
        let mut session = obtained();
        session.token.refresh_token = None;

        let path =
            super::write_session(&home, &somewhere(), &session).expect("the session is written");
        let body = std::fs::read_to_string(&path).expect("the file is there");

        assert!(!body.contains("refreshToken"), "{body}");
        assert!(body.contains("accessToken"), "{body}");
    }

    #[test]
    fn writing_leaves_no_temporary_file_behind() {
        let home = tempdir();
        let path = super::write_session(&home, &somewhere(), &obtained()).expect("written");
        let directory = path.parent().expect("parent");

        let names: Vec<String> = std::fs::read_dir(directory)
            .expect("the cache directory")
            .map(|entry| {
                entry
                    .expect("entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();

        assert_eq!(
            names.len(),
            1,
            "exactly the session file remains: {names:?}"
        );
    }

    #[test]
    fn a_second_sign_in_replaces_the_first_without_a_moment_of_absence() {
        // The rename is what buys this: at no point is the file missing or
        // half-written, which matters because the AWS CLI reads the same one.
        let home = tempdir();
        let at = somewhere();
        super::write_session(&home, &at, &obtained()).expect("first");

        let mut second = obtained();
        second.token.access_token = super::SignInSecret::new("second-access-token");
        let path = super::write_session(&home, &at, &second).expect("second");

        let body = std::fs::read_to_string(&path).expect("the file is there");
        assert!(body.contains("second-access-token"), "{body}");
        assert!(
            !body.contains(r#""accessToken":"access-token-value""#),
            "{body}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn the_session_file_is_readable_only_by_its_owner() {
        use std::os::unix::fs::PermissionsExt as _;

        let home = tempdir();
        let path = super::write_session(&home, &somewhere(), &obtained()).expect("written");
        let mode = std::fs::metadata(&path)
            .expect("metadata")
            .permissions()
            .mode();

        assert_eq!(mode & 0o777, 0o600, "a bearer token is not world-readable");
    }

    /// A directory of this test's own, removed by the OS rather than by us:
    /// the crate has no `tempfile` dependency and this does not justify one.
    fn tempdir() -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "caixonho-sso-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("a directory to write into");
        path
    }

    #[tokio::test]
    async fn it_waits_through_pending_answers_and_returns_the_session() {
        let port = SsoSignInDouble::pending_then_issues(2);
        let waiter = Recorded::new();

        let outcome = sign_in(&port, &waiter, &somewhere(), &Abandon::default(), |_| {})
            .await
            .expect("the double issues a token");

        assert!(matches!(outcome, SignInOutcome::Session(_)));
        assert_eq!(port.polls(), 3, "two pending answers, then the token");
    }

    #[tokio::test]
    async fn the_authorization_is_shown_before_any_waiting() {
        // The one ordering that matters to a user: the code has to be on
        // screen while they are being waited for, not once the wait is over.
        let port = SsoSignInDouble::pending_then_issues(1);
        let waiter = Recorded::new();
        let seen: Mutex<Option<String>> = Mutex::new(None);

        sign_in(&port, &waiter, &somewhere(), &Abandon::default(), |auth| {
            *seen.lock().expect("seen") = Some(auth.user_code.clone());
            assert!(
                waiter.waits().is_empty(),
                "nothing should have been waited on yet"
            );
        })
        .await
        .expect("the double issues a token");

        assert_eq!(seen.lock().expect("seen").as_deref(), Some("WXYZ-1234"));
    }

    #[tokio::test]
    async fn being_told_to_slow_down_widens_the_wait() {
        let port = SsoSignInDouble::slows_down_then_issues().interval(Duration::from_secs(5));
        let waiter = Recorded::new();

        sign_in(&port, &waiter, &somewhere(), &Abandon::default(), |_| {})
            .await
            .expect("the double issues a token");

        assert_eq!(
            waiter.waits(),
            vec![Duration::from_secs(5), Duration::from_secs(10)],
            "the second wait is the first plus the five seconds RFC 8628 names"
        );
    }

    #[tokio::test]
    async fn the_interval_the_provider_asked_for_is_honoured() {
        let port = SsoSignInDouble::pending_then_issues(1).interval(Duration::from_secs(11));
        let waiter = Recorded::new();

        sign_in(&port, &waiter, &somewhere(), &Abandon::default(), |_| {})
            .await
            .expect("the double issues a token");

        assert_eq!(
            waiter.waits(),
            vec![Duration::from_secs(11), Duration::from_secs(11)],
            "a provider that asks for eleven seconds is not polled every five"
        );
    }

    #[tokio::test]
    async fn a_declined_authorization_stops_at_once_and_says_so() {
        let port = SsoSignInDouble::declined();
        let waiter = Recorded::new();

        let error = sign_in(&port, &waiter, &somewhere(), &Abandon::default(), |_| {})
            .await
            .expect_err("the user declined");

        assert!(
            matches!(
                error,
                Error::SignIn {
                    problem: SignInProblem::Declined,
                    ..
                }
            ),
            "got {error:?}"
        );
        assert_eq!(port.polls(), 1, "no reason to ask a second time");
    }

    #[tokio::test]
    async fn an_attempt_that_outlives_its_window_ends_as_expired_without_polling_again() {
        // The window closes before the second poll would happen. The loop must
        // notice on its own rather than asking the provider for an error it
        // was already told to expect.
        let port = SsoSignInDouble::pending_then_issues(5)
            .interval(Duration::from_secs(60))
            .expires_in(Duration::from_secs(90));
        let waiter = Recorded::new();

        let error = sign_in(&port, &waiter, &somewhere(), &Abandon::default(), |_| {})
            .await
            .expect_err("the attempt expired");

        assert!(
            matches!(
                error,
                Error::SignIn {
                    problem: SignInProblem::Expired,
                    ..
                }
            ),
            "got {error:?}"
        );
        assert_eq!(port.polls(), 1, "polling stops at the window, not after it");
    }

    #[tokio::test]
    async fn the_provider_may_be_the_one_that_says_the_attempt_expired() {
        // Two ways to learn the same thing, and both have to end the same. The
        // test above is our clock noticing; this is the provider saying so
        // first, which happens whenever its idea of the deadline is stricter
        // than ours.
        let port = SsoSignInDouble::attempt_expired();
        let waiter = Recorded::new();

        let error = sign_in(&port, &waiter, &somewhere(), &Abandon::default(), |_| {})
            .await
            .expect_err("the provider says it expired");

        assert!(
            matches!(
                error,
                Error::SignIn {
                    problem: SignInProblem::Expired,
                    ..
                }
            ),
            "got {error:?}"
        );
    }

    #[tokio::test]
    async fn an_unreachable_provider_is_a_network_cause_not_a_sign_in_one() {
        let port = SsoSignInDouble::unreachable();
        let waiter = Recorded::new();

        let error = sign_in(&port, &waiter, &somewhere(), &Abandon::default(), |_| {})
            .await
            .expect_err("the provider was unreachable");

        assert!(matches!(error, Error::Network { .. }), "got {error:?}");
    }

    #[tokio::test]
    async fn registering_is_where_a_failure_can_happen_before_anything_is_shown() {
        let port = SsoSignInDouble::cannot_register();
        let waiter = Recorded::new();
        let shown = Mutex::new(false);

        let error = sign_in(&port, &waiter, &somewhere(), &Abandon::default(), |_| {
            *shown.lock().expect("shown") = true;
        })
        .await
        .expect_err("registration failed");

        assert!(matches!(error, Error::Network { .. }), "got {error:?}");
        assert!(
            !*shown.lock().expect("shown"),
            "nothing is shown for an attempt that never began"
        );
        assert_eq!(port.polls(), 0);
    }

    #[tokio::test]
    async fn abandoning_stops_the_loop_and_produces_no_session() {
        let port = SsoSignInDouble::pending_then_issues(10);
        let abandon = Abandon::default();
        let waiter = {
            let abandon = abandon.clone();
            Recorded::new().doing(move |_| abandon.now())
        };

        let outcome = sign_in(&port, &waiter, &somewhere(), &abandon, |_| {})
            .await
            .expect("abandoning is not a failure");

        assert!(matches!(outcome, SignInOutcome::Abandoned));
        assert_eq!(
            port.polls(),
            0,
            "abandoning during the first wait means the provider is never asked"
        );
    }
}

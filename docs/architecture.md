# Architecture

How the pieces fit and why they are separated that way. The rules here are
enforced — [`AGENTS.md`](../AGENTS.md) lists them as invariants, and breaking
one is a bug even when the code compiles.

## Two crates, one direction

```mermaid
flowchart LR
    GUI["caixonho-gui<br/><i>window, table, theming</i>"]
    CLI["caixonho-cli<br/><i>planned, v3</i>"]
    CORE["caixonho-core<br/><i>all product logic</i>"]
    SDK["aws-sdk-s3"]
    S3["S3 / S3-compatible<br/>endpoint"]

    GUI --> CORE
    CLI -.-> CORE
    CORE --> SDK
    SDK --> S3

    GUI x--x SDK
```

`caixonho-core` owns every decision: which credentials to use, what a failure
means, what may be claimed about permissions, what to fetch and when. The
frontend renders and reports input.

The crossed line is the rule that keeps this honest: **the GUI never depends on
`aws-sdk-s3`.** When it needs to show something AWS-shaped, core re-exports a
domain type for it. Without that rule the UI slowly accumulates product logic
in the shape of SDK types, and the CLI that comes later has to reimplement it.

## Inside core

```mermaid
flowchart TB
    subgraph entry ["Entry point"]
        SESSION["session<br/><i>the long-lived context</i>"]
    end
    subgraph resolve ["Getting a connection"]
        PROFILES["profiles<br/><i>reads ~/.aws</i>"]
        CONNECTION["connection<br/><i>provider chain, region</i>"]
        TLS["tls<br/><i>one HTTP client, OS trust store</i>"]
    end
    subgraph work ["Talking to S3"]
        STORE["store<br/><i>the port: a trait</i>"]
        ADAPTER["adapter<br/><i>the only module naming an SDK type</i>"]
        CLASSIFY["classify<br/><i>failure → cause</i>"]
    end
    subgraph knowing ["Knowing what is allowed"]
        CAPABILITY["capability<br/><i>observations, three-valued</i>"]
        PROBE["probe<br/><i>what to ask about next</i>"]
    end

    SESSION --> PROFILES & CONNECTION & STORE & CAPABILITY & PROBE
    CONNECTION --> TLS
    STORE -.implemented by.-> ADAPTER
    ADAPTER --> CLASSIFY
    PROBE --> STORE
    PROBE --> CAPABILITY
```

`store` is a trait — the *port* — and `adapter` is its one real implementation.
Everything else depends on the trait, which is what makes the specs' scenarios
unit-testable: a hand-written double returns a canned success or any specific
failure, with no AWS account and no network. It is also what leaves the door
open for S3-compatible services behind the same operations.

## Nothing network-shaped runs on the render thread

```mermaid
sequenceDiagram
    participant U as User
    participant W as Window (GPUI)
    participant S as Session
    participant T as tokio runtime
    participant A as S3

    U->>W: pick a profile
    W->>S: spawn_listing(id, profile, deliver)
    S->>T: hand off
    W-->>U: renders "listing…" immediately
    T->>A: open connection, ListBuckets
    A-->>T: buckets, or a failure
    T->>W: deliver(outcome) over a channel
    W-->>U: rows, or the cause and its fix
```

The window never awaits. Work is handed to a tokio runtime on background
threads, and results come back as messages over a channel, applied on GPUI's
executor. Each result is tagged with the connection it belongs to, so a late
answer from a profile the user has left is dropped rather than rendered as if
it belonged to the new one.

Probing uses the same bridge in the other direction: the table reports the rows
on screen, and each settled probe is announced back over a channel.

## Capability is observed, never declared

S3 exposes no API that enumerates what a caller may do. Everything the app says
about permissions is therefore evidence it has collected, and the model is built
so that *not knowing* is a first-class state rather than a rounding error.

```mermaid
stateDiagram-v2
    [*] --> Unobserved
    Unobserved --> Probing: row comes on screen
    Probing --> Allowed: the call succeeded
    Probing --> Denied: an explicit denial code
    Probing --> Unobserved: any other failure
    Unobserved --> Allowed: a real operation succeeded
    Allowed --> Unobserved: credentials changed
    Denied --> Unobserved: credentials changed
```

Three things about this diagram carry most of the product's promise:

- **Only an explicit denial reaches `Denied`.** An expired token, a rejected
  key, an unreachable network, a wrong region and a throttled request each keep
  their own cause and record nothing. Mapping any of them to "access denied"
  sends the user to rewrite an IAM policy that was never the problem — the
  single most common failure of other clients.
- **`Probing` is not in the model.** The three states above are claims about the
  world; being probed is a fact about our own activity, so the in-flight set
  lives beside the model and the view combines them. Without the distinction,
  rows flicker between "unknown" and "denied" as answers land.
- **Changing credentials discards everything.** Observations belong to the
  credentials that produced them, and a probe that lands after a profile switch
  is refused rather than attributed to whatever replaced them.

Probes are cheap (`ListObjectsV2` for at most one key), never destructive, and
never automatic for writes — a write probe would create an object. They are
issued only for rows on screen, a few at a time, so an account with hundreds of
buckets does not become hundreds of requests at startup.

## Errors stay structured

Failures are a typed enum all the way to the frontend, never a string. The
window matches on the cause to choose what to say and which action to offer;
a stringified error can only be printed. This is the mechanism behind the
"honest about permissions" claim, and it is why the classifier is a module of
its own with its own tests.

## Where to go next

- The contracts these components must satisfy: [`../openspec/specs/`](../openspec/specs/)
- Why the UI stack is what it is, with measurements: [`adr/0001-ui-framework.md`](adr/0001-ui-framework.md)
- What is built and what is next: [`roadmap.md`](roadmap.md)

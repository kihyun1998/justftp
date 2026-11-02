# Shipping a blocking and an async client from one codebase

Research for issue #2. Question: what ways exist to ship both a blocking and an
async FTP client from one codebase, and what does each cost in duplication,
dependencies and API surface? A later ticket picks sync, async or both by
prototyping call sites; another decides whether a protocol layer is shared with
a future FTP server. So each approach below also notes how it affects
splitting out a reusable protocol core.

Researched 2026-11-02.

## Short answer

There are four approaches in use:

1. **Hand-maintained parallel copies** (suppaftp).
2. **Keyword-stripping macros** (maybe-async picks one mode per build; bisync
   and the include-the-module-twice trick give both at once).
3. **A runtime-abstraction trait**. This covers several async runtimes, not
   sync plus async (quinn's `Runtime`).
4. **A sans-IO protocol core** that does no I/O, with thin blocking and async
   drivers on top (quinn-proto, rustls).

Only the sans-IO core gives both APIs without copying protocol logic, keeps
the dependency on a runtime out of the core, and gives a server something to
reuse. Its cost is a state-machine design up front and a driver per I/O
flavour. For FTP the drivers stay small, because FTP is request/reply with
one command in flight at a time.

## Sources and how they were read

Read raw (cloned or fetched, then read in the source):

- suppaftp, `main` at `18e0a88` (2026-10-15), version 12.2.1: workspace and
  crate `Cargo.toml`, `src/lib.rs`, `src/async_ftp.rs`, `src/sync_ftp.rs`,
  `src/async_ftp/{tokio_ftp,smol_ftp}.rs`, the three `control.rs` files,
  `src/command.rs`, `CHANGELOG.md`, `CLAUDE.md`.
- maybe-async-rs, `main` at `9f7869c` (2026-05-15), version 0.2.11:
  `Cargo.toml`, the crate docs in `src/lib.rs`; issue
  [fMeow/maybe-async-rs#6](https://github.com/fMeow/maybe-async-rs/issues/6)
  and its comments.
- bisync: README of [JM4ier/bisync](https://github.com/JM4ier/bisync) and
  its crates.io API record. I did not read its source.
- quinn, `main` at `2c57501` (2026-10-31): `quinn-proto/Cargo.toml`,
  `quinn-proto/src/lib.rs` docs, the `Connection` methods in
  `quinn-proto/src/connection/mod.rs`, `quinn/Cargo.toml`,
  `quinn/src/runtime/mod.rs`.
- rustls: `rustls/src/conn/unbuffered.rs` at tag `v/0.23.31`; `main` at
  `8dca43e` (version `0.24.0-dev.2`), `rustls/src/conn/mod.rs` and
  `rustls/Cargo.toml`.
- libunftp, `master`: the file tree, `src/server/controlchan/codecs.rs` and
  `src/server/controlchan/line_parser/parser.rs` (imports and signatures
  only).

Nothing below comes from blog posts or summaries. Where a statement is my
own inference rather than something a source says, it is marked as such.

## 1. Hand-maintained parallel copies: suppaftp

**What it is.** suppaftp keeps three full client implementations side by side:

| Tree | Files (raw `wc -l`, tests and comments included) |
|---|---|
| sync | `sync_ftp.rs` 3225, `sync_ftp/` 962 → about 4.2k |
| tokio | `async_ftp/tokio_ftp.rs` 3208, `tokio_ftp/` 1302 → about 4.5k |
| smol | `async_ftp/smol_ftp.rs` 3394, `smol_ftp/` 1329 → about 4.7k |

No macro generates them. A search for `macro_rules`, `include!`,
`maybe_async` and `duplicate` across `src/` finds nothing. The repository's
own `CLAUDE.md` says: "Async implementations mirror the same API with
`async`/`await`". Each tree has 47–48 public methods (`pub fn` / `pub async
fn` at impl indentation). `cwd` reads the same in both, apart from `.await`
and `control().await?` (`sync_ftp.rs:374`, `tokio_ftp.rs:396`).

What is shared across the three: `command.rs` (typed `Command` enum,
CR/LF injection checks), `list.rs` (LIST/MLSD parsing), `status.rs`,
`types.rs`, `encoding.rs`, `regex.rs`. What is *not* shared: reading and
parsing replies. `read_response_in` and `code_from_buffer` exist three times,
in `sync_ftp/control.rs:125,231` and in each async `control.rs:282`.

**Duplication, measured.** I removed `async `, `.await`, leading whitespace
and blank lines, then ran `diff`:

- `sync_ftp.rs` (2947 lines) vs `tokio_ftp.rs` (2855): 2130 lines differ
  (counting both sides).
- `tokio_ftp.rs` vs `smol_ftp.rs` (3036): 723 lines differ.
- the sync vs tokio `control.rs`: 313 of 360 + 411 lines differ.
- the tokio vs smol `control.rs`: 58 differ.

The files contain in-file tests, so these counts mix library and test
differences. So the copies are not mechanical copies of each other. They have
drifted in structure. The changelog shows the cost in practice: nearly every
fix entry says "the sync, tokio and smol clients" (for example the reply
size limit, the data-connection accept fix, CR/LF validation, `set_encoding`).
Every fix lands three times.

**Behaviour that cannot match.** Async Rust has no async `Drop`. When a
`TransferStream` is dropped, the sync client reads the 226/250 reply at once,
while "tokio, smol" "defer the reply to the next command" (CHANGELOG, entry
`feat!: make data streams finalize themselves`). Parallel copies are where
this difference ends up.

**Dependencies and features.** No extra dependency for sync. The async side
needs `async-trait` and `pin-project` plus one of tokio/smol. TLS multiplies
the features: `native-tls`, `rustls-{aws-lc-rs,ring}`,
`{tokio,smol}-async-native-tls`, `{tokio,smol}-rustls-{aws-lc-rs,ring}`, plus
`-vendored` variants. That is 18 features besides `default` in
`crates/suppaftp/Cargo.toml`.
Some cannot be on together; `lib.rs:170` has
`compile_error!("smol-async-native-tls and tokio-async-native-tls are mutually exclusive")`.

**API surface.** Three parallel public type families: `suppaftp::FtpStream`,
`suppaftp::tokio::AsyncFtpStream`, `suppaftp::smol::AsyncFtpStream`, each
with its own `*TlsStream` trait, connector types and `cfg`-gated re-exports.
`async_ftp.rs` is mostly `#[cfg]`/`#[cfg_attr(docsrs, …)]` re-export
plumbing.

**Protocol core.** Partial. Commands and listing parsing are shared and
IO-free. The reply reader and the command sequencing (which reply codes each
operation expects, the order of PASV, data connect and RETR) live inside each
I/O tree. A server could reuse `Command` rendering but not the sequencing.

## 2. Keyword-stripping macros

### maybe-async: one mode per build

**What it is.** A proc macro. You write async code. With the Cargo feature
`is_sync` on, `#[maybe_async]` removes `async` and `.await` and the code
compiles as blocking. `#[sync_impl]` / `#[async_impl]` mark the bodies that
really differ, such as socket calls (crate docs, `src/lib.rs`).

**The limit that matters here.** It produces sync *or* async in one build,
never both. A crate cannot offer `Client` and `AsyncClient` side by side with
it. Cargo features are unified across the dependency graph, so if any crate in
a program turns `is_sync` on, every user of maybe-async in that program gets
sync. The docs recommend `resolver = "2"` as a fix. In issue #6 a user shows
that resolver 2 does not cover this case (it only splits features for
target-specific, build/proc-macro and dev dependencies), with a reproduction
repo. The rspotify maintainers reached the same conclusion. The maintainer
wrote that it "doesn't make sense to compile both async and sync in the same
program". He also said that an attempt at a macro that generates both
(`amphi`) was "really hard to maintain and debug" because it "deprives
compiler from the ability to tell us which lines goes wrong". The issue is
still open.

**Cost.** Dependencies: `syn` (full, visit-mut), `quote`, `proc-macro2` at
build time. In `Send` mode it also emits `#[async_trait]`, so `async-trait`
is needed too, unless you use the `AFIT` mode. Duplication: low wherever the
code really is the same; every I/O call still needs a `sync_impl`/`async_impl`
pair. API surface: one set of names whose signatures change with a feature
flag. That breaks the rule that Cargo features should be additive.

### bisync and the include-twice trick: both at once

**What it is.** bisync's README shows the same source file mounted as two
modules with `#[path = "."] pub mod asynchronous { use
bisync::asynchronous::*; mod inner; }` and the same again for `blocking`.
`#[bisync]` strips or keeps `async` per module, and `#[only_sync]` /
`#[only_async]` mark the bodies that differ. That gives `crate::blocking::Client`
and `crate::asynchronous::Client` in one build.

**Cost.** Low duplication and both APIs at once. But the crate is yanked and
deprecated: its README says the author "does not wish to be a software supply
chain risk for a single, tiny utility" and suggests vendoring. Compiler errors
point into macro-expanded code (the same complaint the maybe-async maintainer
made). The protocol logic is still compiled twice and still mixed with I/O,
so this does nothing for a reusable core.

## 3. A runtime-abstraction trait (async runtimes only)

**What it is.** quinn hides tokio and smol behind `pub trait Runtime` (`new_timer`,
`spawn`, `wrap_udp_socket`) plus `AsyncTimer` and `AsyncUdpSocket`
(`quinn/src/runtime/mod.rs`). The adapters are small: `tokio.rs` 111 lines,
`smol.rs` 105. The features are `runtime-tokio` and `runtime-smol`.

**Relevance.** This removes suppaftp's tokio/smol duplication (the 723-line
gap above), but it does not give a blocking API. It works together with
approach 4 and does not replace it.

## 4. A sans-IO protocol core with thin drivers

**What it is.** The protocol is a state machine that takes bytes and time as
input and returns bytes to send and events as output. It never touches a
socket or a clock.

- **quinn-proto.** Its crate docs say it "contains a fully deterministic
  implementation of QUIC protocol logic. It contains no networking code and
  does not get any relevant timestamps from the operating system". It also
  names "a different event loop than the one tokio provides" and C bindings
  as reasons to use it. The driving API on `Connection` is `handle_event`,
  `handle_timeout(now)`, `poll_transmit`, `poll_timeout`, `poll`,
  `poll_endpoint_events`. Its `[dependencies]` include no async runtime and
  no `futures`. Sizes: quinn-proto `src` about 33.3k lines; the tokio/smol
  driver crate `quinn/src` about 7.1k.
- **rustls.** The 0.23 "unbuffered" API (`conn/unbuffered.rs`, tag
  `v/0.23.31`) has the caller pass `incoming_tls: &mut [u8]` to
  `process_tls_records`. It returns a `ConnectionState` that says what to do
  next: `ReadTraffic`, `EncodeTlsData`, `TransmitTlsData`, `WriteTraffic`,
  and others. On `main` (`0.24.0-dev.2`) the core `Connection` goes further:
  `read_tls(input: &mut dyn TlsInputBuffer, tls: &mut Vec<u8>)` and
  `write(plaintext, tls: &mut Vec<u8>)` only fill buffers ("Any data appended
  to `tls` should be sent to the peer"). Its `[dependencies]` contain no I/O
  or async crate. Blocking and async TLS wrappers (rustls's own `Stream`,
  tokio-rustls, futures-rustls) are built as drivers over this core. I did not
  read their source for this note.
- **libunftp (FTP server, for comparison).** Its control-channel parser is
  already IO-free: `line_parser::parse<T>(line: T) -> Result<Command>` takes
  bytes and returns a `Command`. `FtpCodec` wraps it in a
  `tokio_util::codec::{Decoder, Encoder}` (`controlchan/codecs.rs`). But it is
  private, under `src/server/`, so nothing outside libunftp can reuse it.

**Cost.**

- Duplication: protocol logic once. Each I/O flavour (blocking `std::net`,
  tokio, smol) needs a driver loop that moves bytes between socket and core.
  *Inference, not measured:* for an FTP client that loop is short. Only one
  command is in flight on the control channel. The data channel is a plain
  byte pipe, which the driver can hand to the user as `Read`/`AsyncRead`. So
  most of the 47 methods become "core emits command line, driver writes it,
  driver feeds reply lines back until the core says done".
- Dependencies: the core needs none of tokio, smol, `async-trait` or
  `pin-project`. Each driver carries its own, behind its own feature or crate.
- API surface: two choices. (a) Expose only the drivers (`blocking::Client`,
  `tokio::Client`), so users never see the core. (b) Also publish the core,
  as quinn-proto does, which commits to a second public API that is harder to
  keep stable.
- Design effort: it has to be built this way from the start. FTP's command
  sequencing (PASV/EPSV, then data connect, then RETR, then 150, then the
  transfer, then 226; AUTH TLS, PBSZ, PROT from RFC 4217) must be written as
  explicit states instead of straight-line `?` code. TLS upgrade in the middle
  of a stream (AUTH TLS) stays in the driver, because the TLS crate owns that
  socket. *Inference:* the core still has to *signal* "upgrade now".
- Drop behaviour: the core can track "transfer finished, reply owed", so each
  driver resolves it the same way. That removes the kind of sync/async
  difference suppaftp has. *Inference.*

**Protocol core.** This is the only approach that produces one directly. The
`Command` and `Reply` types, reply-line framing and the multi-line reply rule
(RFC 959 §4.2) are shared by client and server. A server could depend on the
same core crate for parsing commands and rendering replies.

## Comparison

| | Protocol logic copies | Both APIs in one build | Extra deps on the core | Public API shape | Reusable by a server |
|---|---|---|---|---|---|
| Parallel copies (suppaftp) | 1 per flavour (3) | yes | n/a (each copy has its own) | parallel type families, many TLS × runtime features | only `Command`/listing parsers |
| maybe-async | 1 | **no**, sync *or* async by feature; graph-wide unification | `syn`/`quote` (build), `async-trait` unless AFIT | one set of names, signatures change with a feature | no (still mixed with I/O) |
| bisync / include-twice | 1 source, compiled twice | yes | macro crate (bisync yanked; vendor) | `blocking::` and `asynchronous::` modules | no |
| Runtime trait (quinn `Runtime`) | 1 across async runtimes | async only | runtime adapters behind features | one async API | n/a |
| Sans-IO core + drivers | 1 | yes | none in the core | driver types per flavour; core optionally public | **yes** |

## What this means for justftp

These are inferences for the later tickets, not decisions:

- If the call-site prototype ticket picks **sync only** or **async only**, the
  question goes away for now. A sans-IO core still keeps the other option, and
  the server option, open at little extra cost.
- If it picks **both**, maybe-async is out, because it cannot ship both in one
  build. The realistic choices are the sans-IO core or the include-twice
  trick. Parallel copies are what suppaftp shows to be costly (every fix three
  times, behaviour drift).
- For the protocol-sharing ticket: only the sans-IO approach gives a layer a
  server could use. libunftp's IO-free parser shows the server side naturally
  splits that way too.

## Not verified

- I did not build or run any of the crates. The duplication numbers come from
  a textual diff after stripping `async`/`.await`, and they include in-file
  tests.
- I did not read bisync's source or the `amphi` crate, only the README and
  the maintainer's remarks.
- rustls `main` is unreleased (`0.24.0-dev.2`). I did not check whether the
  0.23 unbuffered API survives into 0.24 in its current form.
- The size of an FTP sans-IO driver is an estimate, not a measurement. The
  call-site prototype ticket can measure it.

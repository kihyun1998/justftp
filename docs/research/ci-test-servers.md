# Which FTP servers can the tests run against?

Research for issue #10. Question: which real FTP servers can run in CI to test
the justftp client (vsftpd, Pure-FTPd and ProFTPD in Docker, libunftp
in-process), and which of them cover FTPS, MLSD, EPSV and active mode?

Researched 2026-11-02.

## Answer

Use two kinds of server, because no single one covers the v1 scope on every
CI runner.

1. **libunftp, in-process**, as the default test server. It runs inside
   the test process with no Docker (run on Windows here; tested on Linux by
   its own CI; built on macOS by its own CI), and covers login,
   PASV, EPSV (also over IPv6), MLSD/MLST, REST, rename, delete, directory
   ops, explicit FTPS (AUTH TLS) and active mode via `PORT`. It does **not**
   implement `EPRT`, and its `PASV` refuses IPv6 control connections.
2. **Pure-FTPd in Docker** (`stilliard/pure-ftpd`), on the Linux runner only,
   as the independent real-world server. It implements every v1 command,
   including MLSD, EPRT and EPSV, and FTPS via `ADDED_FLAGS=--tls=…`.
   vsftpd (`delfer/alpine-ftp-server`, what suppaftp uses) is a fine second
   real server but has **no MLSD**. ProFTPD has everything, but no maintained
   image ships FTPS ready to switch on.

Active mode (`PORT`/`EPRT`) does not work against a container on Docker's
default bridge network: the server sees the bridge gateway as the client
address and connects back into the container, not to the test. Test active
mode against libunftp in-process (PORT) and against a Docker server only with
host networking (EPRT), or with scripted in-process fixtures as suppaftp does.

## Coverage matrix

"Yes (source)" means read in the server's own source or docs; see the
evidence sections below.

| Feature              | libunftp 0.24 (in-process)         | Pure-FTPd        | vsftpd 3.0.5     | ProFTPD 1.3.9    |
|----------------------|------------------------------------|------------------|------------------|------------------|
| Runs without Docker  | Yes, any OS with tokio             | No (Linux image) | No (Linux image) | No (Linux image) |
| Explicit FTPS        | Yes (`ServerBuilder::ftps`)        | Yes (`--tls`)    | Yes (`ssl_enable`) | Needs `mod_tls`, not in the maintained image |
| MLSD / MLST          | Yes                                | Yes              | **No**           | Yes (`mod_facts`) |
| EPSV                 | Yes, IPv4 and IPv6                 | Yes              | Yes              | Yes              |
| PASV                 | Yes, IPv4 only (drops an IPv6 session) | Yes              | Yes              | Yes              |
| PORT                 | Yes, opt-in (`ActiveAndPassive`)   | Yes              | Yes              | Yes              |
| EPRT                 | **No**                             | Yes              | Yes              | Yes              |
| REST (resume)        | Yes (`REST STREAM` with `unftp-sbe-fs`) | Yes         | Yes              | Yes              |

## Evidence

### How suppaftp tests (read raw)

Read from a clone of `veeso/suppaftp` at `18e0a88` (upstream `main` HEAD on
2026-11-02, checked with `gh api`).

- `.github/workflows/ci.yml`: the `test` job runs on `ubuntu-latest` only, for
  each feature set. A separate `test-local` job runs on `windows-latest` and
  `macos-latest` and calls `just test_local`.
- `just/test.just`: `test_local` runs `cargo test -p suppaftp --lib --
  slow_data_tests active_peer_tests`, with the comment "Run the tests that
  need no FTP server container (active mode data connections). CI runs these
  on Windows and macOS, where Docker is not available."
- `crates/suppaftp/src/test_container.rs`: servers are started with the
  `testcontainers` crate (0.28, workspace `Cargo.toml`), not with GitHub
  Actions service containers. Both the plain and the TLS server use the
  image `delfer/alpine-ftp-server:latest` (vsftpd). The struct names
  `AsyncPureFtpRunner`/`SyncPureFtpRunner` are historical; the image is
  vsftpd. The TLS variant copies in its own `vsftpd.conf`, a fixture
  certificate and key, and sets `ssl_tlsv12=YES`, `ssl_tlsv13=YES`,
  `require_ssl_reuse=NO`.
- Passive ports 30000-30009 are exposed and mapped to random host ports, so
  the address in the PASV/EPSV reply is wrong from the host's side. The tests
  work around it with `passive_stream_builder`, which rewrites the port with
  `container.get_mapped_port(port)` (`tokio_ftp.rs` around line 1740).
- MLSD against vsftpd is tested as a failure: `should_return_ftps_rejections_and_recover`
  expects `ftp.mlsd(None)` to return `Status::BadCommand` (`tokio_ftp.rs`
  line 1469, `sync_ftp.rs` line 1436). MLSD success is tested only against
  scripted in-process fixtures (`sync_ftp.rs` line 2750).
- Active mode is tested against in-process scripted servers
  (`active_mode_fixture::serve_active_nlst`, `ScriptedDataServer`,
  `serve_port_command`), never against the container. The tests
  `active_mode_listens_on_the_control_connection_address` loop over IPv4 and
  IPv6 control addresses and `continue` when binding fails, so they tolerate
  a runner without `::1`.

### libunftp (read raw, and run)

Read from a clone of `bolcom/libunftp` at upstream `master` HEAD `bdf2402`
(2026-10-30). Crate version 0.24.0, license Apache-2.0, edition 2024.

- In-process use: `tests/common.rs` builds a `ServerBuilder` with
  `unftp_sbe_fs::Filesystem`, then `tokio::spawn(server.listen("127.0.0.1:2150"))`
  inside the test. `listen` takes any `SocketAddr` string
  (`src/server/ftpserver.rs` line 938), so `[::1]:port` is accepted.
- Commands: one handler file per command in
  `src/server/controlchan/commands/`: `abor acct allo appe auth ccc cdup cwd
  dele epsv feat help list md5 mdtm mkd mlsd mlst mode nlst noop opts pass
  pasv pbsz port prot pwd quit rest retr rmd rnfr rnto site size stat stor
  stou stru syst type_ user`. There is no `eprt` handler and no match for
  "EPRT" anywhere in `src/`.
- FEAT (`feat.rs`): always `SIZE MDTM UTF8 EPSV MLST MLSD`; `AUTH TLS PBSZ
  PROT` when FTPS is configured; `REST STREAM` when the storage back end has
  `FEATURE_RESTART`, which `unftp-sbe-fs` 0.4.0 sets (`crates/unftp-sbe-fs/src/lib.rs`
  line 105).
- Active mode: off by default. `ActivePassiveMode` defaults to `PassiveOnly`
  and a `PORT` then gets `502 Active mode not enabled.`
  (`options.rs` line 343, `active_passive.rs`). Enable it with
  `.active_passive_mode(ActivePassiveMode::ActiveAndPassive)`.
- `PORT` (`port.rs`) parses an IPv4 address only (`SocketAddrV4`) and
  connects to the client **while handling PORT**, before the transfer
  command arrives. Most servers connect after RETR/LIST/NLST. A client that
  binds its listener before sending PORT still works (the connection waits in
  the backlog), but a test against libunftp does not exercise the usual
  ordering.
- `PASV` (`pasv.rs`) logs "local address is ipv6!" when the control
  connection is IPv6 (in the run below the server then closed the control
  connection); `EPSV` replies `229 Entering Extended Passive Mode (|||port|)` and
  binds on the control connection's own address family
  (`passive_common.rs`, `TcpSocket::new_v6` for IPv6).
- FTPS: `ServerBuilder::ftps(certs_file, key_file)`, plus `ftps_required`,
  `ftps_tls_flags`, `ftps_client_auth` (`ftpserver.rs` lines 400-500). It
  uses rustls 0.23. No implicit-FTPS option was found (grep for "implicit"
  in `src/` returned nothing).
- libunftp's own CI (`.github/workflows/rust.yml`) runs tests on
  `ubuntu-latest` and only builds on `windows-latest` and `macos-latest`.

**Run on Windows 11 (this machine), 2026-11-02.** A throwaway binary
depending on `libunftp = "0.24"` and `unftp-sbe-fs = "0.4"` started two
servers on one tokio runtime (`127.0.0.1:2121` and `[::1]:2122`, root a temp
directory holding `hello.txt` = `0123456789abcdef`,
`ActiveAndPassive`, default anonymous authenticator) and drove them with raw
commands over `std::net::TcpStream`. Replies seen, identical on both
addresses unless noted:

- `220`, `USER anonymous` → `331`, `PASS x` → `230`, `TYPE I` → `200`.
- `EPSV` → `229 Entering Extended Passive Mode (|||port|)`; `MLSD` → `150`,
  data `type=file;size=16;modify=…; hello.txt`, then `226`.
- `REST 10` → `350`, `EPSV`, `RETR hello.txt` → data `abcdef`, `226`.
- IPv4 only: `PORT 127,0,0,1,…` → `200`, `NLST` → `150`, the server
  connected from `127.0.0.1` and sent `hello.txt`, `226`.
- `EPRT |1|127.0.0.1|40000|` → `500 Command not implemented`.
- `PASV` on IPv4 → `227 … (127,0,0,1,…)`. `PASV` on `[::1]` → no reply; the
  server closed the control connection.

A first run with the whole Windows temp directory as root got
`550 Permission denied` on MLSD and a panic in `unftp-sbe-fs` `cap_fs.rs:56`
(`SendError`), so tests should give the server its own fresh directory.

### Pure-FTPd (read raw)

- Commands, from `src/ftp_parser.c` on `jedisct1/pure-ftpd` `master`
  (fetched raw): `auth` (TLS), `port`, `eprt`, `epsv`, `rest`, `mlst`, `mlsd`
  are all parsed (lines 358-649).
- FEAT, from `src/ftpd.c` `dofeat()`: `UTF8 EPRT IDLE MDTM SIZE MFMT REST
  STREAM MLST … MLSD PRET`, plus `AUTH TLS PBSZ PROT` when built `WITH_TLS`.
- Image `stilliard/pure-ftpd` (README fetched raw from
  `stilliard/docker-pure-ftpd`): passive ports default to 30000-30009,
  `PUBLICHOST` sets the PASV address, `FTP_USER_NAME`/`FTP_USER_PASS`/`FTP_USER_HOME`
  create a user, `ADDED_FLAGS=--tls=1|2` enables TLS, and a self-signed
  certificate is generated if `TLS_CN`, `TLS_ORG` and `TLS_C` are set.
- Docker Hub API (2026-11-02): last pushed 2026-11-01, about 40.5 million
  pulls. The most actively maintained of the images checked.

### vsftpd (read raw)

- Source: vsftpd 3.0.5 tarball. `postlogin.c` dispatches `EPSV`, `EPRT`,
  `REST`, `PORT`; `prelogin.c` handles `AUTH`. "MLSD" appears in no `.c`
  file: vsftpd has no MLSD or MLST.
- `postlogin.c` lines 990-1004 and 1739-1753: unless `port_promiscuous=YES`,
  a PORT or EPRT whose address differs from the control connection's peer is
  refused with `500 Illegal PORT command.` / `Illegal EPRT command.`
- Image `delfer/alpine-ftp-server` (README and `start_vsftpd.sh` fetched raw
  from `delfer/docker-alpine-ftp-server`): env `USERS`, `ADDRESS`
  (pasv_address), `MIN_PORT`/`MAX_PORT` (default 21000-21010), and
  `TLS_CERT`/`TLS_KEY`, which turn on `ssl_enable` and force TLS for logins
  and data. Docker Hub: last pushed 2025-03-01, about 5.5 million pulls.
  The image's shipped config uses `listen=YES`, which is IPv4 only (suppaftp
  copies the same line into its TLS config).

### ProFTPD (read raw, partly)

- `mod_facts.c` (a copy of the ProFTPD source file downloaded by an earlier
  session; grepped here, "MLSD" appears 5 times) implements MLSD and MLST. EPRT/EPSV are in the core.
  FTPS is `mod_tls`.
- No official image. `instantlinux/proftpd` is maintained (Docker Hub push
  2026-10-20; Dockerfile `FROM alpine:3.24`, ProFTPD `1.3.9c-r0`), but its
  Dockerfile installs `proftpd` and `proftpd-mod_sftp` only, not
  `proftpd-mod_tls`, and its env vars have no TLS switch. FTPS would need a
  derived image that adds the Alpine package `proftpd-mod_tls` (it exists in
  Alpine edge, version 1.3.9e) plus a config file under `/etc/proftpd.d`.
  `PASV_ADDRESS` is required.

### GitHub Actions runners (read raw)

- GitHub docs, reusable `docker-container-os-support.md` (fetched raw from
  `github/docs`): "If your workflows use Docker container actions, job
  containers, or service containers, then you must use a Linux runner", and
  on GitHub-hosted runners that means Ubuntu.
- Service containers reached from a job on the runner use
  `localhost:<port>` with explicit port mapping
  (`use-docker-service-containers.md`).
- `actions/runner-images` READMEs: Ubuntu 24.04 lists Docker Server 28;
  Windows 2025 lists Docker 29.7.2; the macOS 15 arm64 image lists only Docker
  Compose and Buildx, not a Docker server.

## Why active mode breaks in Docker

With a published port on the default bridge network the test connects to
`127.0.0.1:<mapped>`. The client's control socket has local address
`127.0.0.1`, so a client that announces its control address sends
`PORT 127,0,0,1,…`. Inside the container, the control peer is the bridge
gateway (typically `172.17.0.1`). vsftpd therefore refuses the PORT (address
differs from the peer, see above); a server that accepts it connects to its
own loopback, where nothing listens. Announcing the gateway address instead
needs the test to know it and the runner's firewall to let the container
reach the host. The two clean options are:

- **Host networking** (`docker run --network host`, Linux only): the server
  and the test share the host's interfaces, so PORT/EPRT to `127.0.0.1` or
  `::1` work, PASV ports need no mapping, and IPv6 loopback is available
  without enabling IPv6 in the Docker daemon. testcontainers-rs has
  `ImageExt::with_network(name)` (source `image_ext.rs` line 86); whether its
  port lookup works with network `host` was not checked.
- **In-process servers**: libunftp for PORT, scripted fixtures for EPRT and
  for edge cases, as suppaftp does.

The same mapped-port problem hits PASV/EPSV: suppaftp rewrites the port in
the client. Fixed one-to-one mappings (`-p 30000-30009:30000-30009`, or
testcontainers `with_mapped_port`) or host networking avoid the client-side
hook.

## Suggested CI layout

- All three OSes: `cargo test` with libunftp in-process (plain and FTPS,
  PASV/EPSV, MLSD, REST, PORT; EPSV on `[::1]` where the runner binds it) and
  scripted fixtures for EPRT and malformed replies.
- `ubuntu-latest` only: an interop job against Pure-FTPd and vsftpd in
  Docker, ideally with host networking so that EPRT and IPv6 are exercised
  against real servers. Add ProFTPD only if a derived image with `mod_tls`
  is accepted.

## Not verified

- libunftp in-process was run on Windows only. On Linux its own CI runs its
  tests; on macOS its CI only builds it. FTPS against libunftp was not run.
- Nothing was run in Docker: this machine has no Docker. The Docker-side
  claims come from source and README reading, not from a run.
- Whether GitHub service containers accept `--network host` through
  `options:`, and whether testcontainers-rs port lookup works on network
  `host`.
- Whether Pure-FTPd and vsftpd accept IPv6 inside the images as shipped
  (vsftpd needs `listen_ipv6=YES`; Pure-FTPd's run script was not read for
  this).
- Whether `::1` is bound on every GitHub-hosted runner.
- That the Windows runner's Docker runs Windows containers only (implied by
  the GitHub note above, not tested).

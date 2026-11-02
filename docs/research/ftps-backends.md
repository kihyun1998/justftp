# What do FTPS backends and servers require?

Research for issue #7. It feeds the later ticket that decides v1 FTPS scope: explicit only or implicit too, and which TLS backend(s) sit behind which feature flags.

Researched 2026-11-02. "Raw" below means the primary text or source code was read directly (RFC text, crate source from crates.io tarballs, vendor docs). "Summary" means the claim came through a search-result summary or a third party, and is weaker.

## Answer

- **Session reuse is a hard requirement in practice.** vsftpd (`require_ssl_reuse`, default YES), ProFTPD (since 1.3.3rc1, unless `TLSOptions NoSessionReuseRequired`) and FileZilla Server (0.9.51+ setting, still enforced in 1.x) refuse a TLS data connection that does not resume the control connection's TLS session. RFC 4217 explicitly allows this.
- **rustls provides it with no extra work.** Its default `ClientConfig` keeps an in-memory session cache keyed by server name. Every connection built from the same `Arc<ClientConfig>` with the same server name resumes from it, covering TLS 1.2 (session ID or ticket) and TLS 1.3 (tickets).
- **native-tls cannot provide it.** It exposes no session API, and on each platform its `connect` path rules resumption out (details below). suppaftp's maintainer closed both reuse issues with "rustls supports session reuse, native-tls doesn't".
- **Implicit FTPS (port 990) is not part of RFC 4217.** It came from pre-RFC drafts and survives only as an IANA port registration and as an option in servers and clients. I found no measured prevalence figure and give none.

## 1. What RFC 4217 says

Read raw: https://www.rfc-editor.org/rfc/rfc4217.txt

- **Implicit FTPS is absent.** The word "implicit" does not occur in the text (grep count 0). Section 3: "an FTP session is established on the normal control port. A client requests TLS with the AUTH command". Section 16 names only port 21: "{FTP-PORT} - The port assigned to the FTP control connection is 21."
- **The server may insist on session reuse.** Section 10.2: "It is reasonable for the server to insist that the data connection uses a TLS cached session. This might be a cache of a previous data connection or of a cleared control connection. If this is the reason for the refusal to allow the data transfer, then the '522' reply should indicate this." It continues: "Note: This has an important impact on client design". Section 18 repeats the point as a scalability benefit.
- **The one exception is REIN.** Section 13: after REIN, "It MAY be acceptable to use cached TLS sessions for subsequent connections, however, a server MUST NOT mandate this."
- **Where implicit FTPS came from:** draft-murray-auth-ftp-ssl-06, read raw (https://www.ietf.org/archive/id/draft-murray-auth-ftp-ssl-06.txt), has section 4.2 "Implicit Session Security". In it the server listens on a distinct port `{FTP-TLSPORT}` and the client starts TLS as soon as it connects. The same draft describes `AUTH SSL` / `TLS-P` as implicit *data channel* protection, kept "primarily for backward compatibility". The published RFC dropped both and kept only `TLS` / `TLS-C`, where section 17 says "the server SHOULD accept 'TLS-C' as a synonym for 'TLS'". I did not track down which draft revision removed section 4.2.
- **IANA still registers the ports.** Read raw from the service-names CSV (https://www.iana.org/assignments/service-names-port-numbers/service-names-port-numbers.csv): `ftps 990/tcp "ftp protocol, control, over TLS/SSL"` and `ftps-data 989/tcp`. The registrant is a person, Christopher_Allen, and no RFC is referenced.

## 2. Which servers require TLS session reuse

| Server | Requirement | How to relax it | Source, how read |
|---|---|---|---|
| vsftpd | `require_ssl_reuse`, **default YES** (since v2.1.0). The doc says it "may break many FTP clients". The error clients see is `522 SSL connection failed; session reuse required: see require_ssl_reuse option in vsftpd.conf man page` | `require_ssl_reuse=NO` | vsftpd.conf man page, raw: https://security.appspot.com/vsftpd/vsftpd_conf.html. The error text is quoted in suppaftp issue #55 (raw), seen against the public test server dlptest.com |
| ProFTPD mod_tls | "As of ProFTPD 1.3.3rc1, mod_tls only accepts SSL/TLS data connections that reuse the SSL session of the control connection, as a security measure. Unfortunately, there are some clients (e.g. curl) which do not reuse SSL sessions." | `TLSOptions NoSessionReuseRequired` | mod_tls docs, raw: http://www.proftpd.org/docs/contrib/mod_tls.html |
| FileZilla Server 0.9.x | Setting "Require TLS session resumption on data connection when using PROT P", reported on by default from 0.9.51 | Uncheck the setting | Summary only (third-party help pages via search). Not read from FileZilla's own docs |
| FileZilla Server 1.x | Enforced. Error `425 Unable to build data connection: TLS session of data connection not resumed`. suppaftp issue #93 (raw) shows FileZilla Server 1.9.2 rejecting suppaftp + native-tls with "TLS session of data connection not resumed". With TLS 1.3 the FileZilla developer is quoted: "FileZilla Server sends NewSessionTicket in response to EPSV/PASV/EPRT/PORT" | **Not verified** whether 1.x has an off switch. I found none, and the FileZilla forum returned HTTP 403 to my fetch | suppaftp #93 (raw). Rebex forum thread https://forum.rebex.net/19415 (raw page, quoting FileZilla's developer). SmartFTP KB 2782 (raw) |
| libunftp (server) | README lists "Explicit FTPS (TLS)" and "TLS session resumption". It does not say whether reuse is *required* | n/a | README, raw: https://github.com/bolcom/libunftp |

**Why the servers want this.** Reuse proves that the data-connection peer knows the same master secret as the authenticated control connection. Without it, a third party who races the client to the PASV port can take over the data connection. vsftpd's own wording: "which proves that they know the same master secret as the control channel". The FileZilla-side argument about session stealing came to me only through summaries.

**A TLS 1.3 detail for the client design.** In TLS 1.3 a resumption ticket arrives as a post-handshake `NewSessionTicket` message on the control connection. FileZilla Server sends it in response to PASV/EPSV/PORT/EPRT, before the 227/229 reply. A client that reads the control reply through its TLS layer processes the ticket before it opens the data connection. Rebex hit a case where the ticket was missing and added a FileZilla-specific workaround. That thread also points at a possible GnuTLS bug (gitlab.com/gnutls/gnutls/-/issues/1303), which I did not read.

## 3. Which backends can provide reuse

### rustls: yes, by default

Read raw: rustls 0.23.46 source (crates.io tarball), `src/client/client_conn.rs` and `src/client/handy.rs`.

- `ClientConfig.resumption: Resumption`. `impl Default for Resumption` calls `Self::in_memory_sessions(256)` when `std` is on, and `Self::disabled()` without `std`. Its doc says: "Create an in-memory session store resumption with up to 256 server names, allowing a TLS 1.2 session to resume with a session id or RFC 5077 ticket."
- `Resumption::store(Arc<dyn ClientSessionStore>)` takes a custom store. `Tls12Resumption` picks the TLS 1.2 mechanism, with `SessionIdOrTickets` as the default.
- `ClientSessionStore` is keyed by `ServerName`. It holds one TLS 1.2 session per server name, and TLS 1.3 tickets that are "return[ed] ... _at most once_". `ClientSessionMemoryCache` keeps up to `MAX_TLS13_TICKETS_PER_SERVER = 8` TLS 1.3 tickets per server.
- **What a client must do:** build the data connection's `ClientConnection` from the *same* `Arc<ClientConfig>` and the *same* `ServerName` as the control connection. suppaftp 12.2.1 does exactly this (`src/sync_ftp/tls/rustls.rs`: `ClientConnection::new(Arc::clone(&self.connector), server_name)`, using the domain stored at `into_secure`), and its maintainer says reuse works.
- **One consequence:** each TLS 1.3 data connection uses up one ticket, so the client depends on the server issuing new ones. FileZilla issues one per PASV/PORT. I have not checked what vsftpd and ProFTPD (both on OpenSSL) do over many consecutive transfers under TLS 1.3. That should be tested against real servers (see the CI test-server ticket).

### native-tls: no, on any platform

Read raw: native-tls 0.2.18 source, plus the backend crates it calls (schannel 0.1.30, security-framework 3.7.0, openssl 0.10.82), and the vendor docs named per row.

- **Public API:** grep for `session|resum|cache` in `src/` finds nothing relevant, so no session API is exposed. Upstream issue rust-native-tls#308 "TLS Session Resumption support" (opened 2024-11-05) is **open with 0 comments** (read raw via the GitHub API).

| Platform backend | What native-tls does | Why that rules out reuse | Source |
|---|---|---|---|
| SChannel (Windows) | `connect` calls `SchannelCred::builder()...acquire(Direction::Outbound)`, so it gets a new credential handle on every connection (`src/imp/schannel.rs`) | Microsoft's InitializeSecurityContext (Schannel) docs say "The cached session is used only if all of the following conditions are met: ... The credential handle is the same." A new handle per connect means no resumption | Raw: https://learn.microsoft.com/en-us/windows/win32/secauthn/initializesecuritycontext--schannel |
| Security.framework (macOS/iOS) | `ClientBuilder::new()` and never calls `enable_session_tickets` (`src/imp/security_framework.rs`). It also caps TLS 1.3 down to 1.2 ("the latest that is actually supported - 1.2") | In security-framework, `enable_session_tickets` defaults to `false`. Only when it is enabled does the builder call `set_peer_id(domain)`, which keys the resumable session | Raw: security-framework 3.7.0 `src/secure_transport.rs` |
| OpenSSL (Linux etc.) | `SslConnector::configure()...connect(domain, stream)` with no `set_session` (`src/imp/openssl.rs`) | The OpenSSL docs say client caching "is not activated by default", and even with it on, "the application must select the session to be reused by using the SSL_set_session(3) function" | Raw: https://docs.openssl.org/3.0/man3/SSL_CTX_set_session_cache_mode/ |

**Consequence for justftp:** a native-tls backend could only resume sessions by dropping to the platform crates (`openssl`, `schannel`, `security-framework`) directly and reimplementing what native-tls does on each OS. Done through native-tls itself, FTPS fails against default vsftpd, ProFTPD ≥ 1.3.3 and FileZilla Server.

### Other differences that matter for an FTP client

- **Build.** rustls is pure Rust with a choice of crypto provider. suppaftp offers `rustls-ring` and `rustls-aws-lc-rs`. native-tls links the system TLS stack (OpenSSL needs system headers on Linux unless the `vendored` feature is on). This comes from suppaftp's feature list and README, read raw.
- **TLS 1.3.** rustls supports it. native-tls on Security.framework stops at 1.2 (see the table).
- **Certificates.** rustls needs a root store from the caller, and does not use the OS store unless a verifier crate such as `rustls-platform-verifier` is added. native-tls uses the OS trust store. This is general knowledge; I did not re-read either crate for this point.
- **IP-literal hosts.** FTP servers are often reached by IP, and both backends verify the name you pass. suppaftp #87 asks about IP/certificate mismatch; I read its title only.

## 4. How suppaftp handles FTPS

Read raw: suppaftp 12.2.1 crates.io source, and GitHub issues #55 and #93 via `gh`.

- Explicit FTPS via `into_secure(connector, domain)`. Implicit via `connect_secure_implicit(addr, connector, domain)`, documented with the warning: "mind that implicit ftps should be considered deprecated, if you can use explicit mode with `into_secure()`".
- Backends are behind features: `native-tls`, `native-tls-vendored`, `rustls-ring`, `rustls-aws-lc-rs`, plus async variants for smol and tokio (`*-async-native-tls`, `*-rustls-*`).
- Session reuse is never handled explicitly. It works on rustls because one `Arc<ClientConfig>` is shared. #55 (vsftpd 522, 2023) and #93 (FileZilla Server 1.9.2, 2024) were both closed with that answer.
- It handles a TLS 1.3 shutdown detail. After an upload, the client drains the data connection until EOF before dropping it, because unread TLS 1.3 session tickets would otherwise make the OS reset the socket and the server answer 426 (`src/async_ftp/smol_ftp/transfer_stream.rs`).

## 5. How common implicit FTPS is

**No measured prevalence figure was found, and none is given here.** The evidence below is qualitative:

- The standard (RFC 4217) defines only explicit FTPS. Implicit FTPS exists through pre-RFC drafts and the IANA port 990 registration (section 1).
- Servers still ship it as a separate, opt-in mode. vsftpd `implicit_ssl` defaults to NO ("To support explicit SSL and/or plain text too, a separate vsftpd listener process should be run"). ProFTPD's `TLSOptions UseImplicitSSL` comes with the advice to put it on its own VirtualHost on port 990. Both read raw.
- Clients still offer it. curl uses implicit FTPS for `ftps://` URLs (port 990 by default) and says "This is usually not how FTPS is done." (https://everything.curl.dev/ftp/ftps.html, read raw through a page fetch). suppaftp offers it but calls it deprecated.
- Implementation cost in a client is small: TLS on connect instead of after `AUTH TLS`, then the same `PBSZ 0` / `PROT P` sequence. This is my inference from the draft text and the suppaftp API; it has not been built.

## Not verified

- Whether FileZilla Server 1.x has any setting to turn the reuse requirement off. The FileZilla forum returned HTTP 403, and the 0.9.x setting is known only through summaries.
- How vsftpd and ProFTPD behave with rustls over TLS 1.3 across many consecutive data connections (ticket supply). Not tested against a live server.
- Which revision of draft-murray-auth-ftp-ssl dropped implicit session security.
- Any number on how many deployed servers use implicit FTPS or require reuse.

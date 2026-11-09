# A sans-IO protocol core under thin blocking and tokio drivers

justftp offers a blocking client and a tokio client with the same method names. Both are thin drivers over one protocol core that touches no socket, clock or runtime: the core says what to send and what to wait for, and a driver moves the bytes. We chose this so the FTP sequencing is written once, the default blocking build has no dependencies, and a later server has a layer to reuse.

The core must be written as explicit states rather than straight-line code, and anything that needs the socket, such as the `AUTH TLS` upgrade, is written once per driver.

## Considered options

- **Parallel copies per runtime, as suppaftp does.** Every fix lands in each copy, and suppaftp's copies already differ in behaviour: its blocking client reads the 226 reply when a transfer stream is dropped, while its async clients defer it to the next command.
- **A macro that strips `async`, such as maybe-async.** It builds one mode per compilation, and Cargo unifies features across the dependency graph, so one crate choosing sync makes the whole program sync.
- **Blocking only, or async only.** Blocking only makes every async caller wrap the client in `spawn_blocking`; async only makes every sync program start a runtime.

Evidence: [the research](https://github.com/kihyun1998/justftp/issues/2) and [the call-site prototype](https://github.com/kihyun1998/justftp/issues/3).

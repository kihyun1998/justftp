# Which listing formats must a client handle?

Research for issue #5. Question: how widely do current FTP servers support MLSD (RFC 3659), and which LIST output formats does a client need to parse to cover the servers it will meet?

Researched 2026-11-02.

## Answer

- MLSD is the only listing format with a specification. Most open-source servers implement it (ProFTPD, Pure-FTPd, legacy FileZilla Server, libunftp). **vsftpd does not**, and says so in its own source tree. vsftpd is a common Linux default, so a client cannot assume MLSD.
- Where the server lacks MLSD, the client falls back to LIST. LIST output has no specification at all (RFC 959). In practice two shapes cover the mainstream servers: **Unix `ls -l`** (vsftpd, ProFTPD, Pure-FTPd, and IIS when configured for it) and **MS-DOS/IIS** (IIS FTP in its default style).
- Everything else (VMS, EPLF, IBM MVS/z/VM, OS/9, HP NonStop, numeric-Unix variants, filename-only lists) exists. FileZilla carries a parser for each, but they are a long tail.
- Detect MLSD from the `MLST` line of the FEAT reply. RFC 3659 defines no separate `MLSD` feature line; some servers print one anyway.
- No source gives a measured share of servers that support MLSD. This file gives none.

## Findings

### What the specifications say

- RFC 959, LIST: "Since the information on a file may vary widely from system to system, this information may be hard to use automatically in a program, but may be quite useful to a human user." (rfc959.txt, section 4.1.3, LIST). There is no LIST format.
- RFC 3659 section 7: MLST/MLSD "differ from the LIST command in that the format of the replies is strictly defined although extensible." Section 7.2: the format "is intended for machine processing, not human viewing, and as such the format is very rigid. Implementations MUST NOT vary the format by ... inserting extra spaces ..., including header or title lines, or inserting blank lines".
- RFC 3659 section 7.8: "Note that there is no distinct FEAT output for MLSD. The presence of the MLST feature indicates that both MLST and MLSD are supported."
- RFC 3659 defines `type=cdir` and `type=pdir` entries (section 7.5.1). A listing can therefore contain entries for the listed directory itself and its parent, and the client must filter them. FileZilla skips them (`ParseAsMlsd` returns 2 for cdir/pdir).
- MLSD `modify` times are UTC. LIST times are whatever the server prints. vsftpd prints GMT unless `use_localtime` is set. FileZilla turns off its timezone-offset guessing once MLST/MLSD is seen ("MLST/MLSD specs require use of UTC", `logon.cpp`).

### Server support for MLSD

| Server | MLSD | LIST shape | Evidence |
|---|---|---|---|
| vsftpd 3.0.5 (latest release per security.appspot.com/vsftpd.html) | **No** | Unix `ls -l` | Source tarball. `REFS`: "vsftpd has not implemented MLST and MLSD due to lack of demand from users." `TODO` lists "MLST, MLSD". `features.c` FEAT has no MLST. `ls.c` builds `perms links owner group size date name`. `sysutil.c` date is `%b %d %H:%M`, or `%b %d  %Y` for dates in the future or older than 6 months. |
| ProFTPD (master) | Yes (`mod_facts`) | Unix `ls -l` | `modules/mod_facts.c`: command table registers `C_MLSD`, `pr_feat_add` for MLST facts, `FactsAdvertise on\|off` directive. Not checked: whether `mod_facts` is built by default (the docs site refused the connection). |
| Pure-FTPd (master) | Yes | Unix `ls -l` | `src/ftpd.c` FEAT: `" MLST type*;size*;sizd*;modify*;UNIX.mode*;UNIX.uid*;UNIX.gid*;unique*;"` and `" MLSD"`. `src/ls.c` writes the Unix form. |
| FileZilla Server 0.9.60 (legacy, SVN trunk) | Yes | not checked | `source/ControlSocket.cpp`: commands MLST/MLSD, FEAT `" MLST type*;size*;modify*;"` and `" MLSD"`. |
| FileZilla Server 1.x | **Not verified** | not verified | Did not find its source or a primary changelog entry. |
| IIS FTP 7.0/7.5+ | **Not verified** | MS-DOS by default, Unix optional | Microsoft Learn `<directoryBrowse>`: `showFlags` value `StyleUnix` "Specifies whether to display UNIX-style directory listings; otherwise, displays MSDOS-style listings", and "There is no default value". With no flag set, the listing is MS-DOS. Other flags: `LongDate` (long dates vs short dates), `UseGmtTime`, `DisplayAvailableBytes`, `DisplayVirtualDirectories`. The same page says "a graphical FTP client might not work with the MS-DOS style". No Microsoft page I found states whether IIS FTP implements MLST/MLSD. |
| libunftp (master) | Yes | not checked | `src/server/controlchan/commands/feat.rs`: `" MLST", " MLSD"` in FEAT. |

### How real clients handle listings

- **suppaftp** (upstream `main`, `crates/suppaftp/src/list.rs`): `list()`, `nlst()`, `mlsd()` return `Vec<String>` raw lines. Parsing is a separate step, and the caller picks the parser: `ListParser::parse_posix`, `parse_dos`, `parse_mlsd`, `parse_mlst`. There is no auto-detection. The module doc says "there's no guarantee this parser works ... There's no specification regarding the LIST command output". Regexes:
  - POSIX: `^([\-ld])([\-rwxsStT]{9})\s+(\d+)\s+([^ ]+)\s+([^ ]+)\s+(\d+)\s+([^ ]+\s+\d{1,2}\s+(?:\d{1,2}:\d{1,2}|\d{4}))\s+(.+)$`
  - DOS: `^(\d{2}\-\d{2}\-\d{2}\s+\d{2}:\d{2}\s*[AP]M)\s+(<DIR>)?([\d,]*)\s+(.+)$`
  - Limits read off these regexes, not tested against servers: only `-`, `l` and `d` types; no `+` or `.` ACL marker after the mode; both owner and group required; a two-digit year only in DOS form, so IIS `LongDate` output with a four-digit year would not match. The exact IIS long-date text is not verified.
- **FileZilla** (`src/engine/directorylistingparser.cpp`, SVN trunk): auto-detects per line. It tries in order: MLSD, Unix with date, DOS, EPLF, VMS, "Other" (numerical Unix, VShell, OS/2), IBM, Wf FTP, IBM MVS (and PDS), OS/9, then MVS-only variants, then Unix without a date. If nothing matches and no line holds a space, it treats the listing as bare filenames. Unparseable lines are dropped. The Unix parser tolerates missing owner/group fields, NetWare split permissions, and several non-English and Asian date forms (`ParseUnixDateTime`, `ParseShortDate`). `ftp/list.cpp` sends `MLSD` when the server capability is set, otherwise `LIST` or `LIST -a`. `ftp/logon.cpp` sets that capability from either an `MLSD` or an `MLST` FEAT line.
- **curl** (`lib/ftplistparser.c`): parses LIST only for wildcard downloads. It implements Unix (three variants plus symlinks) and "DOS style". It picks one per listing from the first character: a digit means Windows NT, anything else means Unix. `lib/ftp.c` never sends MLSD (0 occurrences).
- **Python ftplib** (`Lib/ftplib.py`): `dir()` sends LIST and hands back raw lines with no parsing. `mlsd()` sends MLSD and yields `(name, facts)` tuples. It parses only MLSD.

## Implications for justftp (input to the later "what a listing returns" ticket)

These are inferences from the findings above, not decisions:

- Prefer MLSD when FEAT has `MLST` (also accept a bare `MLSD` line, as FileZilla does). Otherwise fall back to LIST.
- A LIST parser covering Unix `ls -l` and MS-DOS/IIS covers every mainstream server checked here. The long-tail formats are the case for always exposing the raw line.
- Real clients split two ways on unparseable lines: FileZilla drops them silently, and suppaftp returns an error per line and leaves the choice to the caller.

## Sources and how they were read

Read raw (fetched file, read the text):

- RFC 3659 and RFC 959 text, rfc-editor.org.
- vsftpd 3.0.5 source tarball (`REFS`, `TODO`, `features.c`, `ls.c`, `sysutil.c`), security.appspot.com/downloads/vsftpd-3.0.5.tar.gz. Latest-version line from security.appspot.com/vsftpd.html.
- ProFTPD `modules/mod_facts.c`, github.com/proftpd/proftpd master.
- Pure-FTPd `src/ftpd.c` and `src/ls.c`, github.com/jedisct1/pure-ftpd master.
- FileZilla Server 0.9.60 `source/ControlSocket.cpp` and `readme.htm`, svn.filezilla-project.org/svn/FileZilla Server/trunk.
- FileZilla client `src/engine/directorylistingparser.cpp`, `src/engine/ftp/list.cpp`, `src/engine/ftp/logon.cpp`, svn.filezilla-project.org/svn/FileZilla3/trunk.
- suppaftp `crates/suppaftp/src/list.rs` and the `list`/`mlsd` signatures in `sync_ftp.rs`/`async_ftp/*`, github.com/veeso/suppaftp main.
- curl `lib/ftplistparser.c` and `lib/ftp.c`, github.com/curl/curl master.
- CPython `Lib/ftplib.py`, github.com/python/cpython main.
- libunftp `src/server/controlchan/commands/feat.rs`, github.com/bolcom/libunftp master.
- Microsoft Learn: IIS `<directoryBrowse>` configuration reference, "Changes in Microsoft FTP 7.5", "What's New for Microsoft and FTP in IIS 7?". Read in full through a fetch tool. None of them mentions MLST or MLSD.

From search-result summaries only, not relied on for any claim above:

- Forum and Trac posts about FileZilla Server advertising MLSD, and a news item saying FileZilla Server 0.9.4d added MLSD/MLST.

Not verified:

- Whether IIS FTP implements MLST/MLSD.
- Whether FileZilla Server 1.x implements MLSD.
- Whether ProFTPD builds `mod_facts` by default.
- The exact text of IIS `LongDate` listings.
- Any share of deployed servers that support MLSD.

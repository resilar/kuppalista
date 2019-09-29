# kuppalista

Collaborative shopping list app using Rust, WebSockets and JavaScript.

## Usage

Compile backend with `cargo build` to produce `kuppalista` server executable.
Start the server and access the application frontend with a web browser.

```
usage: ./kuppalista [-p password] [-P pkcs12] [--no-prompt] IP:PORT
 e.g.: ./kuppalista -p hunter2 -P tls.p12 127.0.0.1:8080
```

If password is given, then clients must send the correct password as the value
of `Sec-WebSocket-Protocol` header to establish WebSocket connection to the
server. The included HTML/JavaScript frontend accepts the password as
`window.location.hash` so that a properly constructed link (e.g.,
[http://127.0.0.1:8080/#hunter2](http://127.0.0.1:8080/#hunter2)) grants access
to the password-protected shopping list.

PKCS #12 archive file is required for HTTPS. Decryption password for the given
archive file is prompted from the user on startup. If the archive file is not
encrypted, then enter an empty password or set `--no-prompt` flag to disable
the PKCS #12 password prompt.

## Security (or lack thereof)

The app supports pasting formatted HTML into each shopping list item (which is
quite useful because it allows, for example, adding pictures to the list).
However, there is no validation for the user-supplied HTML code which gets
broadcasted and shown to all clients. This has obvious security implications
([Cross-site scripting](https://en.wikipedia.org/wiki/Cross-site_scripting)).

Thus, you probably do not want to make your shopping list public to all
malicious people on the internet. Use HTTPS and strong passwords.

## Screenshot

![Frontend](https://i.imgur.com/JMfIYP7.png)

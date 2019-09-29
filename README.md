# kuppalista

Collaborative shopping list app using Rust, WebSockets and JavaScript.

## Usage

Compile backend with `cargo build` to produce `./kuppalista` server executable.
Start the server and access the application frontend with a web browser.

```
usage: ./kuppalista [-p password] [-b pkcs12] [--no-decrypt] IP:PORT
 e.g.: ./kuppalista -p hunter2 -b tls.p12 127.0.0.1:8080
```

PKCS#12 archive file and password are optional. If password is given, then
clients must send the correct password as the value of `Sec-WebSocket-Protocol`
header to establish WebSocket connection to the server. The included frontend
reads the password from `window.location.hash` so that a properly constructed
link (e.g., [http://127.0.0.1:8080/#hunter2](http://127.0.0.1:8080/#hunter2))
grants access to the password-protected shopping list.

## Security (or lack thereof)

The app supports pasting formatted HTML into each shopping list item (which is
quite useful because it allows, for example, adding pictures to the list).
However, there is no validation for the user-supplied HTML code which gets
broadcasted and shown to all clients. This has obvious security implications
([Cross-site scripting](https://en.wikipedia.org/wiki/Cross-site_scripting)).

Thus, you probably do not want to make your shopping list public to all
malicious people on the internet. Use TLS and password protection.

## Screenshot

![Frontend](https://i.imgur.com/JMfIYP7.png)

# Stream Deck VSD2 (Virtual Stream Deck) — Protocol Encryption Analysis

**Target:** `C:\Program Files\Elgato\StreamDeck\StreamDeck.exe` 7.6.0.23012 (x64, 28,067,016 bytes, MD5 `ef39e7db5f2e358d171a416547e0c512`)
**Date:** 2026-10-20
**Method:** static only — PE parsing, string/RTTI extraction, Capstone cross-reference scan, targeted `llvm-objdump` disassembly, Ghidra 12.1.4 headless import + decompilation. The app was **not** run and no debugger was attached.

**Artifacts**
- Ghidra project: `C:\Users\Flowing\vsd2-ghidra\proj2` (project `VSD2`, program `StreamDeck.exe`, full analysis, auto-saved)
- Decompiler dump used below: `C:\Users\Flowing\vsd2-ghidra\vsd2dump.log`
- Helper scripts: `scan_strings.py`, `ripxref.py`, `rtti.py`, `callscan.py`, `cryptocallers.py`, `Vsd2Dump.java`

All addresses are **VA (image base 0x140000000)**; RVA = VA − 0x140000000.

---

## 1. Executive summary

The post-Hello channel is **libsodium `crypto_box` primitives**, not OpenSSL and not a custom scheme:

| Property | Result | Confidence |
|---|---|---|
| Key agreement | X25519 (`crypto_box_beforenm` = HSalsa20(X25519(sk,pk), 0, "expand 32-byte k")) | **High** |
| Record cipher | XSalsa20-Poly1305 (`crypto_secretbox_easy` / `crypto_secretbox_open_easy`) | **High** |
| Nonce | **24 random bytes prepended to every frame** (fresh per frame, sent in clear) | **High** |
| Wire overhead | **40 bytes/frame** (24 nonce + 16 Poly1305 tag) | **High** |
| Plaintext | the serialized protobuf `vsd2::ClientMessage` / `vsd2::ServerMessage` — nothing else | **High** |
| When it starts | first `ServerMessage` (case 10 = `hello`) and first `ClientMessage` (case 1 = `hello`) are **plaintext**; everything after is encrypted | **High** |
| Server key material | long-term X25519 keypair in an in-process vault slot named `"host"`; public half sent as `HelloFromServer.publicKey` | **Medium-High** |
| Pairing/trust | vault slot `"clients"` = list of trusted 32-byte client public keys; controls `clientNeedsAuthentication` | **Medium** |
| Extra KDF | **none** — the 32-byte `crypto_box_beforenm` output is used directly as the secretbox key | **High** |

The OpenSSL X25519/Poly1305 strings in the binary belong to Qt's statically linked libssl (HTTPS); the VSD2 code path only ever calls the statically linked libsodium block at RVA `0xc98xxx`–`0xcb0xxx`.

**Bottom line for an emulator:** the cipher is trivially re-implementable. Frames are
`nonce[24] || secretbox(plaintext_protobuf)[16-byte tag || ciphertext]`, key = `crypto_box_beforenm(client_sk, server_pk)` = `crypto_box_beforenm(server_sk, client_pk)`.
No pairing secret, PIN, or signature participates in the key schedule — pairing only gates *application-level* authorization (the `clientNeedsAuthentication` flag / Authenticate state machine).

---

## 2. Where the code lives

### 2.1 Strings (RVA / VA)

| RVA | String | Meaning |
|---|---|---|
| 0x16a8280 | `### [VSD2 Server] port %d is not available` | server startup |
| 0x16a8410 | `### [VSD2 Server] new connection` | |
| 0x16a8438 | `ESDVSD2Server::onNewWebSocketConnection` | |
| 0x16a84f0 | `### [VSD2 Server] init with workstationID=%s` | |
| 0x16a8540 | `### [VSD2 Server] sodium_init error` | **libsodium init** |
| 0x16a8568 | `ESDVSD2Server::init` | |
| 0x16a8598 | `C:\builds\5\streamdeck\ESDVSD2Server\ESDVSD2Server.cpp` | PDB path |
| 0x16a8580 (UTF-16) | `VSD2 Server` | HTTP `Server:` header |
| 0x16a8d24 | `ESDVSD2HandshakeHandler` | class name (log/RTTI helper) |
| 0x16a9db8 | `ESDVSD2Connection` | class name |
| 0x16ab108 | `unexpected message received` | dispatcher error |
| 0x15a51df | `HelloFromClient` / 0x15a51f2 `publicKey` / 0x15a5205 `clientInformation` | proto fields |
| 0x15a5795 | `HelloFromServer` / 0x15a57fb `publicKey` / 0x15a580e `clientNeedsAuthentication` | proto fields |
| 0x15a5239 | `Authenticate` / 0x15a5249 `requestID` | pairing message |
| 0x15a6135 | `com.elgato.vsd2` | protobuf package |

### 2.2 RTTI / vtables

`ESDVSD2HandshakeHandler` type descriptor `0x19d97b8`, COLs `0x1725ac0` / `0x1725b60`, vtables **`0x16ab848`** and `0x16ab8a8`.
Other VSD2 RTTI: `ESDVSD2Server` (`0x1930c50`), `ESDVSD2Connection` (`0x1930d40`), `ESDVSD2WebSocketClient`, `ESDVSD2AbstractClient`, `ESDVSD2VirtualDeviceHandler`, `ESDVSD2ContextHandler`, `ESDVSD2ClientCapabilitiesHandler`, `ESDVSD2iOSUSB*`.

### 2.3 Key functions (all `FUN_` in Ghidra because there is no usable PDB; names inferred from RTTI + log strings)

| VA | Size | Inferred identity | Why |
|---|---|---|---|
| `0x14063cd80` | ~0x180 | `ESDVSD2Server::init` | calls `sodium_init` (`0x140c98890`), logs `init with workstationID=%s` |
| `0x14065e8c0` | 0x60 | `ESDVSD2HandshakeHandler::processMessage` | switches on `ClientMessage` case: 1→hello, 2→authenticate, 3→cancel |
| `0x14065e920` | ~0x410 | `processHello(HelloFromClient)` | validates 32-byte publicKey, builds `HelloFromServer` |
| `0x14065ed30` | ~0x330 | `processAuthenticate` | requires state 1, sets state 2, pairing prompt |
| `0x140661f00` | 0xB0 | crypto session ctor / `ensureHostKeypair()` | `crypto_box_keypair` + vault insert |
| `0x140662090` | 0x160 | **`encryptServerMessage(crypto, msg, QByteArray& out)`** | calls `crypto_secretbox_easy` |
| `0x1406621f0` | 0x1C0 | **`decryptClientMessage(crypto, QByteArray& in, msg)`** | calls `crypto_box_beforenm` / `crypto_secretbox_open_easy` |
| `0x140659b60` | 0x90 | vault get `"host"` → 64-byte keypair | key label `0x1418ac520` = `"host"` |
| `0x140659bf0` | 0x90 | vault find in `"clients"` (32-byte match, 64-byte stride) | key label `0x1418ac540` = `"clients"` |
| `0x140659c80` | 0x120 | vault lookup (mutex + FNV-1a hash map) | |
| `0x140659d90` | 0xF0 | vault insert | |
| `0x14065d6b0` | ~0x400 | inbound frame demux (`processInboundData`) | size≥4, looks up client by id, calls `0x1406621f0` |
| `0x14065dab0` | ~0x1B0 | per-connection frame dispatch | `0x140659200` then `0x1406621f0` |

### 2.4 libsodium (statically linked, no names in the PE)

Identified by code shape and callers; the block starts around RVA `0xc97xxx`.

| VA | libsodium function | Evidence |
|---|---|---|
| `0x140c98890` | `sodium_init` | returns −1 (init failure) / 0 / 1, guards `sodium_crit_enter` |
| `0x140c98960` | `sodium_misuse` | calls abort after handler |
| `0x140c989a0` | `sodium_bin2hex` | 0–15 nibble table, hex output |
| `0x140c98ac0` | `randombytes_buf(buf, len)` | calls implementation table at `0x141a11000` |
| `0x140c98f50` | `crypto_scalarmult` (curve25519) | |
| `0x140c98fb0` | `crypto_scalarmult_base` | |
| `0x140c991c0` | **`crypto_box_keypair(pk, sk)`** | `randombytes_buf(sk,32)` then `scalarmult_base(pk,sk)` |
| `0x140c99130` | **`crypto_box_beforenm(k, pk, sk)`** | `scalarmult(q, sk, pk)` → `crypto_core_hsalsa20(k, 0, q, sigma@0x140eac838)` |
| `0x140c992e0` | `crypto_secretbox_detached(c, mac, m, mlen, n, k)` | HSalsa20 + Salsa20 XOR + Poly1305 |
| `0x140c98bb0` | **`crypto_secretbox_easy(c, m, mlen, n, k)`** | tail-calls detached with `c+16` / `c` |
| `0x140c99530` | `crypto_secretbox_open_detached` | |
| `0x140c98c20` | **`crypto_secretbox_open_easy(m, c, clen, n, k)`** | rejects `clen < 16`, calls open_detached |

Only these libsodium call sites exist inside the whole VSD2 code range (verified by a full `.text` call scan): `sodium_init`, `sodium_misuse`, `sodium_bin2hex`, `randombytes_buf`, `crypto_box_keypair`, `crypto_box_beforenm`, `crypto_secretbox_easy`, `crypto_secretbox_open_easy`. There is **no** Ed25519/BLAKE2b/HKDF in the VSD2 session path (the `esd::crypto::scheme::*`, `Blake3_Hasher`, `Signature_Ed25519` classes belong to plugin/asset signing, not to VSD2).

---

## 3. Protocol / framing

### 3.1 Sequence

```
TCP 28198, WebSocket (Server: VSD2 Server), binary frames, 1 protobuf msg per WS frame

client ── ClientMessage{ hello: HelloFromClient{ publicKey: <32B>, clientInformation } }   PLAINTEXT
server ── ServerMessage{ hello: HelloFromServer{ workstationID, hostname, version,
                           protocolVersion, publicKey:<32B>, clientNeedsAuthentication,
                           osName, osVersion, endpoints } }                               PLAINTEXT
        ── (server sets crypto.ready = 1, encrypt-hello-sent = 1)
client ── nonce[24] || secretbox( ClientMessage{ authenticate:{requestID} } )              ENCRYPTED
server ── nonce[24] || secretbox( ServerMessage{ ok | error | virtualDeviceList | ... } )   ENCRYPTED
...
```

- Server-side state machine at `ESDVSD2Connection+0xa8`:
  `0` = expect HelloFromClient, `1` = hello received / unauthenticated, `2` = Authenticate received, pending approval, `3` = established (paired/known client).
  `processHello` sets `state = 2*knownClient + 1` (known → 3, unknown → 1).
- `processAuthenticate` (`0x14065ed30`) only accepts state `1`, stores `requestID` at handler+0x40, moves to state `2`, and if the client is not auto-approved pops the desktop pairing UI (it hex-encodes the 32-byte client key with `sodium_bin2hex` at `0x14065ee79`, 64 hex chars = fingerprint shown to the user). `onPairingAccepted`/`onPairingDenied` strings exist at `0x16a8d3c`/`0x16a8d4f`.
- **Important:** the encryption key is already installed at Hello time (`processHello` → `crypto_box_beforenm` directly). Authentication does *not* change the key.

### 3.2 Frame layout (both directions, after the plaintext hellos)

```
 offset 0            24                 40               40+mlen
 +-------------------+------------------+----------------+
 | nonce (24 bytes)  | Poly1305 tag(16) | ciphertext(m)  |
 +-------------------+------------------+----------------+
   randombytes_buf()   secretbox_easy() writes mac at c[0..16), ct at c[16..]
```

`mlen = frame_len − 40`.

This explains the capture sizes exactly:

| Observed frame | Plaintext protobuf | Reasonable content |
|---|---|---|
| client 45 B | 5 B | `ClientMessage{ authenticate { requestID:"x" } }` = `12 03 0A 01 78` |
| client 47 B | 7 B | same with a 3-char requestID |
| server 103 B | 63 B | `ServerMessage{ ok }` / small `virtualDeviceList` |
| server 254 B | 214 B | `ServerMessage{ virtualDeviceList }` / `virtualDeviceOpened` |
| 34 KB frames | ~34 KB − 40 | `ServerMessage{ icon: Image{ image: ... } }` (PNG/JPEG bytes, `vsd2.ImageFormat`) |

This is why "Deflate and plain protobuf decoding fail": byte 0 is a random nonce, not a protobuf tag, and the payload is XSalsa20 keystream.

### 3.3 Key derivation

```
client: (cpk, csk) = crypto_box_keypair()                 // ephemeral or persisted by the client
        HelloFromClient.publicKey = cpk

server: vault["host"] = (hpk, hsk)                        // long-term, created once per install
        HelloFromServer.publicKey = hpk

both:   K[32] = crypto_box_beforenm(peer_pk, own_sk)
             = HSalsa20( X25519(own_sk, peer_pk), nonce=0, sigma="expand 32-byte k" )
```

`K` is used **directly** as the `crypto_secretbox` key for both directions (same key, independent random nonces). No direction separation, no counter, no transcript hash, no HKDF.

Server code (`0x1406621f0`, plaintext-Hello branch) confirms the argument order:

```c
// FUN_1406621f0, branch crypto->ready == 0  (first inbound frame)
if (msg->case_ == 1 /* hello */ && msg->hello.publicKey.size() == 32) {
    keypair64 = vault_get_host();                 // FUN_140659b60(vault, out64)
    if (crypto_box_beforenm(crypto->key,          // crypto+0x08, 32 bytes
                            msg->hello.publicKey, // client pk
                            keypair64 + 0x20)     // host sk   <-- offset 32
        == 0) {
        crypto->ready = 1;                        // crypto+0x28
        return true;
    }
}
```

### 3.4 Encrypt / decrypt (decompiled)

```c
// 0x140662090  bool encrypt(ESDVSD2Crypto *c, vsd2::SharedServerMessage *m, QByteArray *out)
serialize(*m, &bytes);                       // FUN_140bd45c0
if (!c->ready) return false;                 // crypto+0x28
if (c->hello_sent) {                         // crypto+0x29
    out->resize(bytes.size + 0x28);          // 40 bytes overhead
    randombytes_buf(out->data(), 0x18);      // 24-byte nonce
    rc = crypto_secretbox_easy(out->data() + 0x18, bytes.data, bytes.size,
                               out->data() /*nonce*/, c->key /*+0x08*/);
    return rc == 0;
}
if (m->case_ == 10) {                        // ServerMessage.hello
    c->hello_sent = 1;
    out->append(bytes);                      // plaintext HelloFromServer
    return true;
}
return false;

// 0x1406621f0  bool decrypt(ESDVSD2Crypto *c, QByteArray *in, ClientMessage *out)
if (!c->ready) { ... plaintext hello handling as above ... }
if (in->size() <= 0x27) return false;        // need >= 40
mlen = in->size() - 0x28;
tmp = malloc(mlen);
if (crypto_secretbox_open_easy(tmp, in->data() + 0x18 /*ct*/, in->size() - 0x18,
                               in->data() /*nonce*/, c->key) == 0)
        return out->ParseFromArray(tmp, mlen);
return false;
```

Note `crypto_secretbox_easy` places the **tag first** (`c[0..16)`) and ciphertext after, so the on-wire order is `nonce(24) | tag(16) | ct(mlen)`.

### 3.5 Key store (vault) — how `hpk`/`hsk` and trust are held

`ESDVSD2Connection` owns a mutex-protected `unordered_map<std::string, std::vector<T>>` (vault object at `conn+0x18` for lookups, `conn+0x38` passed to the crypto object at `conn+0xa0`). Its entries are keyed by literal labels visible in `.data`:

| VA | label | contents |
|---|---|---|
| `0x1418ac520` | `"host"` | one 64-byte record = X25519 public key ‖ secret key (server identity) |
| `0x1418ac540` | `"clients"` | vector of 64-byte records, searched by first 32 bytes = trusted client public keys |
| `0x1418ac560` | `"ed25519"` | signing material (unrelated to VSD2 session) |

`FUN_140661f00` (`ensureHostKeypair`) on a `"host"` miss does `crypto_box_keypair(pk, sk)` and inserts `pk‖sk`; `processHello` then answers with `pk`. This means **HelloFromServer.publicKey is stable across reconnects for a given install**, not per-connection — useful as a static-analysis fingerprint and as a dynamic-analysis sanity check.

---

## 4. Candidate pseudo-code (reconstructed)

```c
// ESDVSD2HandshakeHandler::processMessage  @0x14065e8c0
switch (ClientMessage.case_) {
  case 1: processHello(this, msg);        // 0x14065e920
  case 2: processAuthenticate(this, msg); // 0x14065ed30
  case 3: cancelAuthenticate(this);       // 0x14065f180
  default: log("unexpected message received");
}

void processHello(ESDVSD2HandshakeHandler *h, const ClientMessage &m) {
    ESDVSD2Connection *c = h->conn;                 // h+0x18
    if (c->state != 0) { error(c, "HelloFromClient not expected in this state"); return; }
    if (m.hello.publicKey.size() != 32) { error(c, "HelloFromClient invalid public key"); return; }
    memcpy(h->clientPub, m.hello.publicKey.data(), 32);   // h+0x20, std::string
    if (h->clientPub.size() != 32) { error(...); return; }

    bool known = vault_find_clients(c->vault, h->clientPub);   // 0x140659bf0
    keypair64 = vault_get_host(c->vault);                      // 0x140659b60 (pk||sk)

    HelloFromServer hs;
    build_hello(&hs, keypair64, h->clientPub);                 // 0x140145b40
    fill_server_fields(&hs, c->server /*+0x28*/);              // 0x1406520b0
    m = make_shared<ServerMessage>(); m->hello = hs;           // 0x140649b60 / 0x14064a130
    c->send(m);                                                // vtable +0x68  -> encrypt() -> ws
    c->state = known ? 3 : 1;                                  // 2*known + 1
}
```

```c
// 0x140661f00  ESDVSD2Crypto::init(vault)
crypto->vault = vault; crypto->ready = 0; crypto->hello_sent = 0;
if (!vault_find(vault, "host")) {
    uint8_t kp[64];
    crypto_box_keypair(kp, kp + 32);
    vault_insert(vault, "host", kp, 1);
}
```

---

## 5. Answers to the specific questions

1. **Where is the server?** `ESDVSD2Server` (RTTI `0x1930c50`, `.cpp` path string at `0x16a8598`), `init` at `0x14063cd80`; connection/handler classes in the `0x14063c000`–`0x140660400` range; handshake handler at `0x14065e8c0`.
2. **Post-Hello framing/cipher:** `XSalsa20-Poly1305` secretbox, `nonce(24) || tag(16) || ciphertext`, key from X25519 + HSalsa20 (`crypto_box_beforenm`), no additional KDF.
3. **Where do key/nonce come from:** key = `crypto_box_beforenm(client_pub, host_sk)` where `host` is a long-term keypair in the VSD2 vault; nonce = `randombytes_buf(24)` per frame, transmitted in the clear at the front of the frame.
4. **First encrypted message layout:** client's first encrypted frame is `ClientMessage{ authenticate: { requestID } }` (5–7 bytes of protobuf in the observed captures); server's first encrypted frame is a `ServerMessage` (`ok`/`error`/`virtualDeviceList`), 63+ bytes.

---

## 6. Confidence & open questions

| Item | Confidence | Note |
|---|---|---|
| secretbox/XSalsa20-Poly1305 + 24-byte prepended nonce + 16-byte tag | High | decompiled both ends; sizes match captures exactly |
| `crypto_box_beforenm` key, used directly | High | decompiled `0x1406621f0`, `0x140661f00`, `0x140c99130` |
| hello messages plaintext, everything else encrypted | High | explicit `case 10` / `case 1` plaintext branches in `0x140662090`/`0x1406621f0` |
| server `publicKey` = long-term `"host"` key | Medium-High | vault label `"host"` and `keypair64+0x20` (sk) usage; the exact build order inside `0x140145b40`/`0x1406520b0` was not fully decompiled |
| `clientNeedsAuthentication` = "client pk not in `clients`" | Medium | derived from `2*known+1` state assignment; UI pairing path inferred from strings |
| exact protobuf field numbers inside `HelloFromClient`/`HelloFromServer` | Medium | taken from descriptor name ordering (`publicKey` after `clientVersion`-style fields) plus the capture; not read out of the descriptor blobs |
| whether `"clients"`/`"host"` are persisted to disk (and where) | Unknown | `esd::crypto::memory_vault` exists; not traced |

Not investigated (out of timebox): persistence format of the vault, the `ed25519` slot usage, the iOS-USB transport (`ESDVSDVSD2iOSUSB*`), and image payload encoding.

---

## 7. Exact dynamic-analysis next steps (not performed)

**No app run / no debugger was used here.** To confirm end-to-end, hook these (x64 fastcall: `rcx, rdx, r8, r9`, rest on stack):

1. **`0x140c991c0` `crypto_box_keypair(pk=rcx, sk=rdx)`** — log both 32-byte outputs. Confirms the server `"host"` keypair is generated once and reused across reconnects (compare across two sessions).
2. **`0x140c99130` `crypto_box_beforenm(k=rcx, pk=rdx, sk=r8)`** — log all three. `pk` must equal `HelloFromClient.publicKey`; `sk` must equal bytes 32..63 of the `"host"` vault record; `k` is the session key. Dump `k` and then decrypt the stream offline.
3. **`0x140c98bb0` `crypto_secretbox_easy(c=rcx, m=rdx, mlen=r8, n=r9, k=[rsp+0x28])`** — confirms `mlen = frame − 40` and that `m` is a serialized protobuf.
4. **`0x140c98c20` `crypto_secretbox_open_easy`** — dump `m` output to get the exact first encrypted `ClientMessage` (expected `authenticate`/`requestID`) and the server replies.
5. **`0x14063cd80` (`ESDVSD2Server::init`)** — breakpoint after `sodium_init` (`0x14063cdd0`) to catch the `sodium_init` return path; optional.
6. **`0x14065e920` (`processHello`)** — log `[h+0x20]` (client pk), the `"host"` record, and the resulting `[conn+0xa8]` state; this pins the `clientNeedsAuthentication` semantics.
7. **`0x14065ed30` (`processAuthenticate`)** — observe the hex fingerprint (`0x14065ee79`, 64 chars) and the state transition 1→2→3 to map the pairing flow.
8. Cheapest offline validation without hooks: implement `crypto_box_beforenm` + `crypto_secretbox_open_easy` in a scratch client, feed a captured Hello pair (client pk + server pk) **plus the client's private key** — a capture alone is not sufficient because X25519 needs one secret.

Alternative to hooking: run a MITM WebSocket proxy that terminates the connection as a *client* (generating its own keypair), which is enough to read all server->client plaintext because the key schedule has no authentication binding beyond the pairing flag.

---

## 8. Raw evidence snippets

Server init / libsodium:

```
14063cdc8:  call 0x140c98890            ; sodium_init()
14063cdcd:  cmp  eax, -1
14063cdd6:  lea  rdx, [rip+0x106b763]   ; "### [VSD2 Server] sodium_init error"
```

Encrypt path:

```
140662146:  call 0x140c98ac0            ; randombytes_buf(data, 24)
14066214b:  lea  r9, [rbp+8]            ; k = crypto->key
14066214f:  lea  rcx, [rdi+0x18]        ; c = buf + 24
140662153:  mov  [rsp+0x20], r9         ; 5th arg = key
140662158:  mov  r9, rdi                ; n = buf (24-byte nonce)
14066215b:  mov  r8, rbx                ; mlen
14066215e:  mov  rdx, rsi               ; m
140662161:  call 0x140c98bb0            ; crypto_secretbox_easy
```

Decrypt path:

```
140662295:  call 0x140c98a40            ; crypto_box_beforenm(k=crypto+8, pk=client, sk=keypair+0x20)
140662349:  call 0x140c98c20            ; crypto_secretbox_open_easy(m, c=frame+24, clen, n=frame, k)
14066235d:  call 0x140bd4410            ; ClientMessage::ParseFromArray(m, len)
```

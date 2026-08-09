# ERPX patch

This directory vendors `tonic-web-wasm-client` 0.8.0 under its original
MIT/Apache-2.0 license.

ERPX adds the response-body trailer forwarding fix released upstream in 0.9.1:
parsed gRPC-Web trailers are emitted as an `http_body::Frame::trailers` before
end-of-stream. This preserves `grpc-status` for Tonic while retaining
`wasm-streams` 0.4, the version shared by Dioxus 0.7 and reqwest 0.12.
Using upstream 0.9.1 directly introduces `wasm-streams` 0.5 alongside 0.4,
whose generated WebAssembly exports collide at link time.


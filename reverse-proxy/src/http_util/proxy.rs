use std::fmt::Write;

use anyhow::{Error, bail};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub async fn handshake<S: AsyncRead + AsyncWrite + Unpin>(
    stream: &mut S,
    (host, port): (&str, u16),
) -> Result<(), Error> {
    let mut req = String::new();
    writeln!(&mut req, "CONNECT {host}:{port} HTTP/1.1")?;
    writeln!(&mut req, "Host: {host}:{port}")?;
    writeln!(&mut req, "Proxy-Connection: keep-alive")?;
    writeln!(&mut req, "Connection: keep-alive")?;
    writeln!(&mut req)?;
    let req = req.into_bytes();
    stream.write_all(&req).await?;
    stream.flush().await?;

    let mut res = req;
    res.clear();
    let mut lf_count = 0;
    while lf_count < 2 {
        // Read byte-by-byte to avoid buffering.
        // HTTP proxy response is usually very short so syscall overhead is negligible.
        let b = stream.read_u8().await?;
        res.push(b);
        if b == b'\n' {
            lf_count += 1;
        } else if lf_count != 0 && b != b'\r' {
            bail!(
                "HTTP proxy: Expected two newlines in the end of response '{:?}'",
                String::from_utf8_lossy(&res)
            );
        }
    }

    let res = String::from_utf8(res)?;
    let mut iter = res.split_ascii_whitespace();
    if iter.next() != Some("HTTP/1.1") {
        bail!("HTTP proxy: Unsupported response protocol: {res:?}");
    }
    if iter.next() != Some("200") {
        bail!("HTTP proxy: Unsuccessful response status: {res:?}",);
    }

    Ok(())
}

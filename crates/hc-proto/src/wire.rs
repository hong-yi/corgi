//! Framed mode: `[channel u32 LE][kind u8][len u32 LE][payload]`.

use tokio::io::{AsyncRead, AsyncReadExt};

pub const DATA: u8 = 0;
pub const CREDIT: u8 = 1;
pub const CLOSE: u8 = 2;
pub const RESIZE: u8 = 3;
pub const FOCUS: u8 = 4;
pub const FACTS: u8 = 5;
pub const AUTOMATIC_INPUT: u8 = 6;

pub const MAX_PAYLOAD: usize = 16 << 20;
/// Credit every stream starts with.
pub const INITIAL_CREDIT: usize = 256 << 10;
const HEADER: usize = 9;

pub fn encode(channel: u32, kind: u8, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(HEADER + payload.len());
    out.extend_from_slice(&channel.to_le_bytes());
    out.push(kind);
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(payload);
    out
}

pub fn resize_payload(cols: u16, rows: u16) -> [u8; 4] {
    let mut p = [0; 4];
    p[..2].copy_from_slice(&cols.to_le_bytes());
    p[2..].copy_from_slice(&rows.to_le_bytes());
    p
}

pub fn parse_resize(p: &[u8]) -> Option<(u16, u16)> {
    (p.len() == 4).then(|| (u16::from_le_bytes([p[0], p[1]]), u16::from_le_bytes([p[2], p[3]])))
}

/// Read one frame. `Ok(None)` at a clean end of stream between frames.
pub async fn read_frame<R: AsyncRead + Unpin>(r: &mut R) -> std::io::Result<Option<(u32, u8, Vec<u8>)>> {
    let mut head = [0u8; HEADER];
    // A clean EOF is only allowed before the first header byte.
    let n = r.read(&mut head).await?;
    if n == 0 {
        return Ok(None);
    }
    r.read_exact(&mut head[n..]).await?;
    let channel = u32::from_le_bytes(head[0..4].try_into().unwrap());
    let kind = head[4];
    let len = u32::from_le_bytes(head[5..9].try_into().unwrap()) as usize;
    if len > MAX_PAYLOAD {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("frame of {len} bytes"),
        ));
    }
    let mut payload = vec![0; len];
    r.read_exact(&mut payload).await?;
    Ok(Some((channel, kind, payload)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncWriteExt;

    #[tokio::test]
    async fn frames_round_trip_across_arbitrary_splits() {
        let mut bytes = encode(0, DATA, br#"{"id":1}"#);
        bytes.extend(encode(7, RESIZE, &resize_payload(120, 40)));
        bytes.extend(encode(7, CLOSE, b""));
        let (mut a, mut b) = tokio::io::duplex(64);
        let writer = tokio::spawn(async move {
            for chunk in bytes.chunks(3) {
                a.write_all(chunk).await.unwrap();
            }
        });
        assert_eq!(
            read_frame(&mut b).await.unwrap(),
            Some((0, DATA, br#"{"id":1}"#.to_vec()))
        );
        let (ch, kind, p) = read_frame(&mut b).await.unwrap().unwrap();
        assert_eq!((ch, kind, parse_resize(&p)), (7, RESIZE, Some((120, 40))));
        assert_eq!(read_frame(&mut b).await.unwrap(), Some((7, CLOSE, vec![])));
        writer.await.unwrap();
        assert_eq!(read_frame(&mut b).await.unwrap(), None);
    }

    #[tokio::test]
    async fn oversized_and_truncated_frames_are_errors() {
        let mut huge = encode(1, DATA, b"");
        huge[5..9].copy_from_slice(&((MAX_PAYLOAD + 1) as u32).to_le_bytes());
        assert!(read_frame(&mut huge.as_slice()).await.is_err());
        let cut = encode(1, DATA, b"hello");
        assert!(read_frame(&mut &cut[..11]).await.is_err());
    }
}

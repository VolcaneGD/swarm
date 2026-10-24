//! Bounded framing for the future local Named Pipe transport.
//! This crate does not open pipes or infer authority from a connection.
use serde::{Deserialize, Serialize};
use std::io::{ErrorKind, Read, Write};
use swarm_core::protocol::{Fault, Request};

pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_FRAME_BYTES: usize = 8 * 1024 * 1024;

pub const RESPONSE_CHUNK_BYTES: usize = 1024 * 1024;
pub const MAX_RESPONSE_BYTES: usize = 1536 * 1024 * 1024;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RpcResponse {
    pub schema_version: u32,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<Fault>,
}
impl std::fmt::Debug for RpcResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RpcResponse")
            .field("schema_version", &self.schema_version)
            .field("ok", &self.ok)
            .field("payload", &"[redacted]")
            .finish()
    }
}
impl RpcResponse {
    pub fn from_result(result: Result<serde_json::Value, Fault>) -> Self {
        match result {
            Ok(data) => Self {
                schema_version: SCHEMA_VERSION,
                ok: true,
                data: Some(data),
                error: None,
            },
            Err(error) => Self {
                schema_version: SCHEMA_VERSION,
                ok: false,
                data: None,
                error: Some(error),
            },
        }
    }
}

struct ResponseWriter<'a, W: Write> {
    writer: &'a mut W,
    chunk: Vec<u8>,
    total: usize,
}
impl<W: Write> ResponseWriter<'_, W> {
    fn emit(&mut self, more: bool) -> std::io::Result<()> {
        let header = self.chunk.len() as u32 | if more { 1 << 31 } else { 0 };
        self.writer.write_all(&header.to_le_bytes())?;
        self.writer.write_all(&self.chunk)?;
        self.chunk.clear();
        Ok(())
    }
}
impl<W: Write> Write for ResponseWriter<'_, W> {
    fn write(&mut self, mut bytes: &[u8]) -> std::io::Result<usize> {
        let length = bytes.len();
        if length > MAX_RESPONSE_BYTES.saturating_sub(self.total) {
            return Err(std::io::Error::other("Response limit exceeded"));
        }
        self.total += length;
        while !bytes.is_empty() {
            if self.chunk.len() == RESPONSE_CHUNK_BYTES {
                self.emit(true)?;
            }
            let count = bytes.len().min(RESPONSE_CHUNK_BYTES - self.chunk.len());
            self.chunk.extend_from_slice(&bytes[..count]);
            bytes = &bytes[count..];
        }
        Ok(length)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Response chunks have a continuation bit in the length's high bit.
/// JSON may span chunks, including UTF-8 code points; decode only after completion.
pub fn write_response(writer: &mut impl Write, response: &RpcResponse) -> Result<(), Fault> {
    let mut chunks = ResponseWriter {
        writer,
        chunk: Vec::new(),
        total: 0,
    };
    serde_json::to_writer(&mut chunks, response).map_err(|_| disconnected())?;
    chunks.emit(false).map_err(|_| disconnected())?;
    chunks.writer.flush().map_err(|_| disconnected())
}
pub fn read_response(reader: &mut impl Read) -> Result<RpcResponse, Fault> {
    let stream = ResponseReader {
        reader,
        remaining: 0,
        total: 0,
        final_chunk: false,
    };
    let response: RpcResponse =
        serde_json::from_reader(std::io::BufReader::new(stream)).map_err(|error| {
            if error.is_io() && error.io_error_kind() != Some(ErrorKind::InvalidData) {
                disconnected()
            } else {
                invalid()
            }
        })?;
    if response.schema_version != SCHEMA_VERSION
        || (response.ok && (response.data.is_none() || response.error.is_some()))
        || (!response.ok && (response.data.is_some() || response.error.is_none()))
    {
        return Err(invalid());
    }
    Ok(response)
}

struct ResponseReader<'a, R: Read> {
    reader: &'a mut R,
    remaining: usize,
    total: usize,
    final_chunk: bool,
}
impl<R: Read> Read for ResponseReader<'_, R> {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        if bytes.is_empty() {
            return Ok(0);
        }
        if self.remaining == 0 {
            if self.final_chunk {
                return Ok(0);
            }
            let mut header = [0; 4];
            self.reader.read_exact(&mut header)?;
            let header = u32::from_le_bytes(header);
            let length = (header & !(1 << 31)) as usize;
            if length == 0
                || length > RESPONSE_CHUNK_BYTES
                || length > MAX_RESPONSE_BYTES.saturating_sub(self.total)
            {
                return Err(std::io::Error::new(
                    ErrorKind::InvalidData,
                    "Invalid response frame",
                ));
            }
            self.remaining = length;
            self.total += length;
            self.final_chunk = header & (1 << 31) == 0;
        }
        let count = bytes.len().min(self.remaining);
        let read = self.reader.read(&mut bytes[..count])?;
        if read == 0 {
            return Err(std::io::Error::new(
                ErrorKind::UnexpectedEof,
                "Incomplete response frame",
            ));
        }
        self.remaining -= read;
        Ok(read)
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RpcRequest {
    pub schema_version: u32,
    pub request: Request,
}
fn invalid() -> Fault {
    Fault::new("INVALID_FRAME", 2, "Invalid or unsupported IPC frame")
}
fn disconnected() -> Fault {
    Fault::new(
        "CONNECTION_ERROR",
        8,
        "IPC connection failed or ended during a frame",
    )
}

/// Four-byte little-endian byte length followed by one UTF-8 JSON document.
/// Clean EOF is allowed only between frames; an oversized length is rejected
/// before reading or allocating its body.
pub fn read_frame(reader: &mut impl Read) -> Result<Option<Vec<u8>>, Fault> {
    let mut header = [0u8; 4];
    loop {
        match reader.read(&mut header[..1]) {
            Ok(0) => return Ok(None),
            Ok(_) => break,
            Err(e) if e.kind() == ErrorKind::Interrupted => continue,
            Err(_) => return Err(disconnected()),
        }
    }
    reader
        .read_exact(&mut header[1..])
        .map_err(|_| disconnected())?;
    let length = u32::from_le_bytes(header) as usize;
    if length == 0 || length > MAX_FRAME_BYTES {
        return Err(invalid());
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body).map_err(|_| disconnected())?;
    Ok(Some(body))
}

pub fn write_frame(writer: &mut impl Write, bytes: &[u8]) -> Result<(), Fault> {
    if bytes.is_empty() || bytes.len() > MAX_FRAME_BYTES {
        return Err(invalid());
    }
    writer
        .write_all(&(bytes.len() as u32).to_le_bytes())
        .map_err(|_| disconnected())?;
    writer.write_all(bytes).map_err(|_| disconnected())?;
    writer.flush().map_err(|_| disconnected())
}

pub fn read_request(reader: &mut impl Read) -> Result<Option<RpcRequest>, Fault> {
    let Some(bytes) = read_frame(reader)? else {
        return Ok(None);
    };
    let envelope: RpcRequest = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
    if envelope.schema_version != SCHEMA_VERSION {
        return Err(invalid());
    }
    Ok(Some(envelope))
}

pub fn write_request(writer: &mut impl Write, request: &Request) -> Result<(), Fault> {
    #[derive(Serialize)]
    struct Envelope<'a> {
        schema_version: u32,
        request: &'a Request,
    }
    let mut bytes = BoundedBuffer(Vec::new());
    serde_json::to_writer(
        &mut bytes,
        &Envelope {
            schema_version: SCHEMA_VERSION,
            request,
        },
    )
    .map_err(|_| invalid())?;
    write_frame(writer, &bytes.0)
}

struct BoundedBuffer(Vec<u8>);
impl Write for BoundedBuffer {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > MAX_FRAME_BYTES.saturating_sub(self.0.len()) {
            return Err(std::io::Error::other("IPC frame limit exceeded"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Cursor;

    #[test]
    fn chunked_responses_round_trip_large_unicode_and_hide_debug_payload() {
        let response = RpcResponse::from_result(Ok(json!({"body": "引き継ぎ".repeat(200_000)})));
        let mut stream = Vec::new();
        write_response(&mut stream, &response).unwrap();
        assert!(u32::from_le_bytes(stream[..4].try_into().unwrap()) & (1 << 31) != 0);
        let decoded = read_response(&mut Cursor::new(stream)).unwrap();
        assert_eq!(decoded.data, response.data);
        assert!(!format!("{response:?}").contains("引き継ぎ"));
    }
    #[test]
    fn malformed_chunk_lengths_and_incomplete_responses_are_rejected() {
        for header in [0, RESPONSE_CHUNK_BYTES as u32 + 1] {
            assert_eq!(
                read_response(&mut Cursor::new(header.to_le_bytes()))
                    .unwrap_err()
                    .code,
                "INVALID_FRAME"
            );
        }
        assert_eq!(
            read_response(&mut Cursor::new([4, 0, 0, 128, b'{', b'"']))
                .unwrap_err()
                .code,
            "CONNECTION_ERROR"
        );
        let malformed = b"{\"schema_version\":1,\"ok\":true,\"error\":{\"code\":\"X\",\"exit_code\":8,\"message\":\"secret\"}}";
        let mut wire = (malformed.len() as u32).to_le_bytes().to_vec();
        wire.extend_from_slice(malformed);
        assert_eq!(
            read_response(&mut Cursor::new(wire)).unwrap_err().code,
            "INVALID_FRAME"
        );
    }

    fn request() -> Request {
        Request {
            operation: "post.create".into(),
            auth: Some("private-credential".into()),
            args: json!({"thread_id":"thread","body":"引き継ぎ"}),
            context: json!({}),
            request_id: Some("retry-key".into()),
        }
    }
    #[test]
    fn frames_round_trip_multiple_requests_and_preserve_unicode() {
        let mut stream = vec![];
        write_request(&mut stream, &request()).unwrap();
        write_request(&mut stream, &request()).unwrap();
        let mut stream = Cursor::new(stream);
        for _ in 0..2 {
            let envelope = read_request(&mut stream).unwrap().unwrap();
            assert_eq!(envelope.request.args["body"], "引き継ぎ");
            assert_eq!(envelope.request.request_id.as_deref(), Some("retry-key"));
        }
        assert!(read_request(&mut stream).unwrap().is_none());
    }
    #[test]
    fn large_lengths_are_refused_before_consuming_the_body() {
        let mut data = ((MAX_FRAME_BYTES + 1) as u32).to_le_bytes().to_vec();
        data.extend_from_slice(b"unread-body");
        let mut stream = Cursor::new(data);
        assert_eq!(read_frame(&mut stream).unwrap_err().code, "INVALID_FRAME");
        assert_eq!(stream.position(), 4);
        assert!(read_frame(&mut Cursor::new([0u8; 4])).is_err());
    }
    #[test]
    fn oversized_outbound_context_is_refused_without_writing_a_partial_frame() {
        let mut request = request();
        request.context = json!({"oversized": "x".repeat(MAX_FRAME_BYTES)});
        let mut output = vec![];
        assert_eq!(
            write_request(&mut output, &request).unwrap_err().code,
            "INVALID_FRAME"
        );
        assert!(output.is_empty());

        let mut buffer = BoundedBuffer(vec![0; MAX_FRAME_BYTES - 1]);
        assert!(buffer.write_all(&[1, 2]).is_err());
        assert_eq!(buffer.0.len(), MAX_FRAME_BYTES - 1);
    }
    #[test]
    fn maximum_post_body_fits_even_with_json_control_character_escaping() {
        let mut request = request();
        request.args["body"] = json!("\0".repeat(1024 * 1024));
        let mut bytes = vec![];
        write_request(&mut bytes, &request).unwrap();
        assert!(bytes.len() > 6 * 1024 * 1024);
        let decoded = read_request(&mut Cursor::new(bytes)).unwrap().unwrap();
        assert_eq!(
            decoded.request.args["body"].as_str().unwrap().len(),
            1024 * 1024
        );
    }
    #[test]
    fn truncated_header_or_body_is_not_clean_eof() {
        for data in [vec![1], vec![4, 0, 0, 0, b'a', b'b']] {
            assert_eq!(
                read_frame(&mut Cursor::new(data)).unwrap_err().code,
                "CONNECTION_ERROR"
            );
        }
        assert!(read_frame(&mut Cursor::new(vec![])).unwrap().is_none());
    }
    #[test]
    fn invalid_utf8_json_unknown_fields_and_versions_are_refused_without_echoing_data() {
        for bytes in [vec![255],b"private-credential".to_vec(),br#"{"schema_version":2,"request":{"operation":"board.list","args":{},"context":{}}}"#.to_vec(),br#"{"schema_version":1,"extra":"secret","request":{"operation":"board.list","args":{},"context":{}}}"#.to_vec()] {
            let mut stream=vec![];
            write_frame(&mut stream,&bytes).unwrap();
            let fault=read_request(&mut Cursor::new(stream)).unwrap_err();
            assert_eq!(fault.code,"INVALID_FRAME");
            assert!(!fault.message.contains("private-credential"));
        }
    }
    #[test]
    fn debug_does_not_expose_credential_or_body() {
        let envelope = RpcRequest {
            schema_version: 1,
            request: request(),
        };
        let debug = format!("{envelope:?}");
        assert!(!debug.contains("private-credential"));
        assert!(!debug.contains("引き継ぎ"));
    }
    #[test]
    fn failed_write_and_empty_frame_return_faults() {
        assert!(write_frame(&mut vec![], &[]).is_err());
        struct Broken;
        impl Write for Broken {
            fn write(&mut self, _bytes: &[u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("private-detail"))
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let fault = write_request(&mut Broken, &request()).unwrap_err();
        assert_eq!(fault.code, "CONNECTION_ERROR");
        assert!(!fault.message.contains("private-detail"));
    }
}

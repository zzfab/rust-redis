use anyhow::Result;
use bytes::BytesMut;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

#[derive(Clone, Debug)]
pub enum Value {
    SimpleString(String),
    BulkString(String),
    Array(Vec<Value>),
}

impl Value {
    pub fn serialise(self) -> String {
        match self {
            Value::SimpleString(s) => format!("+{}\r\n", s),
            Value::BulkString(s) => format!("${}\r\n{}\r\n", s.chars().count(), s),
            _ => panic!("Not implemented for serialize"),
        }
    }
}

pub struct RespHandler {
    stream: TcpStream,
    buffer: BytesMut,
}

impl RespHandler {
    pub fn new(stream: TcpStream) -> Self {
        RespHandler {
            stream,
            buffer: BytesMut::with_capacity(512),
        }
    }

    pub async fn read_value(&mut self) -> Result<Option<Value>> {
        let bytes_read = self.stream.read_buf(&mut self.buffer).await?;

        if bytes_read == 0 {
            return Ok(None);
        }

        let (v, _) = parse_message(self.buffer.split())?;
        Ok(Some(v))
    }

    pub async fn write_value(&mut self, value: Value) -> Result<()> {
        self.stream.write(value.serialise().as_bytes()).await?;
        Ok(())
    }
}

fn parse_message(buffer: BytesMut) -> Result<(Value, usize)> {
    match buffer[0] as char {
        '+' => parse_simple_string(buffer),
        '$' => parse_bulk_string(buffer),
        '*' => parse_array(buffer),
        _ => Err(anyhow::anyhow!("Not a known value type{:?}", buffer)),
    }
}

fn read_until_crlf(buffer: &[u8]) -> Option<(&[u8], usize)> {
    for i in 1..buffer.len() {
        if buffer[i - 1] == b'\r' && buffer[i] == b'\n' {
            return Some((&buffer[..i - 1], i + 1));
        }
    }
    return None;
}

fn parse_simple_string(buffer: BytesMut) -> Result<(Value, usize)> {
    if let Some((line, len)) = read_until_crlf(&buffer[1..]) {
        let string = String::from_utf8(line.to_vec()).unwrap();

        return Ok((Value::SimpleString(string), len + 1));
    }
    return Err(anyhow::anyhow!("Invalid String{:?}", buffer));
}

fn parse_int(buffer: &[u8]) -> Result<i64> {
    Ok(String::from_utf8(buffer.to_vec())?.parse::<i64>()?)
}

fn parse_bulk_string(buffer: BytesMut) -> Result<(Value, usize)> {
    let (array_length, bytes_consumsed) = if let Some((line, len)) = read_until_crlf(&buffer[1..]) {
        let array_length = parse_int(line)?;
        (array_length, len + 1)
    } else {
        return Err(anyhow::anyhow!("Invalid Array{:?}", buffer));
    };
    let end_of_bulk_str = bytes_consumsed + array_length as usize;

    let total_parsed = end_of_bulk_str + 2;
    Ok((
        Value::BulkString(String::from_utf8(
            buffer[bytes_consumsed..end_of_bulk_str].to_vec(),
        )?),
        total_parsed,
    ))
}

fn parse_array(buffer: BytesMut) -> Result<(Value, usize)> {
    let (array_length, mut bytes_consumed) =
        if let Some((line, len)) = read_until_crlf(&buffer[1..]) {
            let array_length = parse_int(line)?;
            (array_length, len + 1)
        } else {
            return Err(anyhow::anyhow!("Invalid array format {:?}", buffer));
        };
    let mut items = vec![];
    for _ in 0..array_length {
        let (array_item, len) = parse_message(BytesMut::from(&buffer[bytes_consumed..]))?;
        items.push(array_item);
        bytes_consumed += len;
    }
    return Ok((Value::Array(items), bytes_consumed));
}

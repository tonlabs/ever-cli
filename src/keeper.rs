use clap::ArgMatches;
use ever_dc_keeper::engine::messages::MethodName::Write;
use ever_dc_keeper::engine::messages::{MethodName, ReadRequest, ReadResponse, RequestHeader, ResponseHeader, Serializable, WriteRequest, WriteResponse};
use ever_dc_keeper::engine::process_message::{read_all, write_all};
use ton_types::fail;
use crate::{Config, keeper};

async fn send_message<T: Serializable>(stream: &mut tokio::net::TcpStream, name: MethodName, request: T) -> Result<(), String> {
    let mut buffer = Vec::<u8>::new();

    let mut request = request.serialize();
    buffer.append(&mut RequestHeader{ name, len: request.len() as u64 }.serialize());
    buffer.append(&mut request);

    write_all(stream, &buffer).await
        .map_err(|err| format!("Error while sending request to keeper: {}", err))?;

    Ok(())
}

async fn read_response<T: Serializable>(stream: &mut tokio::net::TcpStream) -> Result<T, String> {
    let response_header_binary = read_all(stream, ResponseHeader::size()).await
        .map_err(|err| format!("Error while reading response from keeper: {}", err))?;
    let response_header = ResponseHeader::parse(&response_header_binary)
        .map_err(|err| format!("Error while reading response from keeper: {}", err))?;
    let answer_binary = read_all(stream, response_header.len).await
        .map_err(|err| format!("Error while reading response from keeper: {}", err))?;
    let answer = T::parse(&answer_binary)
        .map_err(|err| format!("Error while parsing response from keeper: {}", err))?;
    Ok(answer)
}

pub(crate) async fn write_binary_to_keeper(binary: &[u8]) -> Result<(), String> {
    let mut stream = tokio::net::TcpStream::connect("127.0.0.1:9680").await
        .map_err(|err| format!("Error while connecting to keeper: {}", err))?;

    send_message(&mut stream, MethodName::Write, WriteRequest{data: binary.to_vec()}).await?;
    let answer = read_response::<WriteResponse>(&mut stream).await?;
    println!("Error code {}", answer.error_code);

    send_message(&mut stream, MethodName::Read, ReadRequest{random: 666}).await?;
    let answer = read_response::<ReadResponse>(&mut stream).await?;
    println!("Data len {}", answer.result.len());

    Ok(())
}


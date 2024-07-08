use std::io;
use std::io::{Error, Read};
use std::process::{exit, Command, Output};

use actix::{Actor, ActorContext, StreamHandler};
use actix_cors::Cors;
use actix_web::body::MessageBody;
use actix_web::{get, post, web, App, Handler, HttpRequest, HttpResponse, HttpServer, Responder};
use actix_web_actors::ws;
use actix_web_actors::ws::{Message, ProtocolError};
use base64::prelude::BASE64_STANDARD;
use base64::Engine;
use serde::Deserialize;

const PROTOCOLS: &[&str] = &["voice_upload"];

#[derive(Debug, Deserialize)]
struct Payload {
    audio_str: String,
}

impl Payload {
    pub fn audio_str(&self) -> &str {
        self.audio_str.as_str()
    }
}

struct MicrophoneWs {
    byte_buf: Vec<u8>,
}

impl MicrophoneWs {
    #[inline]
    fn new() -> Self {
        Self { byte_buf: vec![] }
    }

    #[inline]
    fn append(&mut self, data: &bytes::Bytes) {
        let aa: Result<Vec<u8>, Error> = data.bytes().collect();

        match aa {
            Err(e) => {
                println!("error => {e:#?}");

                exit(1);
            }
            Ok(mut v) => {
                println!("appended bytes : {}", v.len());

                self.byte_buf.append(&mut v);

                println!("after self.byte_buf length : {}", self.byte_buf.len());
            }
        }
    }

    #[inline]
    fn byte_buf(&self) -> &[u8] {
        self.byte_buf.as_slice()
    }
}

impl Actor for MicrophoneWs {
    type Context = ws::WebsocketContext<Self>;
}

impl StreamHandler<Result<Message, ProtocolError>> for MicrophoneWs {
    fn handle(&mut self, item: Result<Message, ProtocolError>, ctx: &mut Self::Context) {
        match item {
            Ok(Message::Ping(item)) => {
                println!("receive ping message");
                ctx.pong(&item);
            }
            Ok(Message::Text(text)) => {
                println!("receive text message => {text:?}");
                ctx.text(text);
                convert_audio_file(&self.byte_buf);

                ctx.stop();
            }
            Ok(Message::Binary(bin)) => {
                println!("receive binary message");
                self.append(&bin);
                ctx.binary(bin);
            }
            Ok(Message::Close(None)) => {
                println!("close connection by client");
            }
            _ => {
                println!("what???");
            }
        }
    }

    fn started(&mut self, ctx: &mut Self::Context) {
        println!("Start connection");
    }

    fn finished(&mut self, ctx: &mut Self::Context) {
        println!("Receive payload finished");
        println!("bytes length => {:#?}", self.byte_buf().len());
        ctx.stop();
    }
}

fn convert_audio_file(buf: &Vec<u8>) {
    println!("Entered convert_audio_file function!");

    let filename = chrono::Utc::now().timestamp_millis();
    let source_name = format!("{}.webm", filename);
    let target_name = format!("{}.mp3", filename);

    std::fs::write(source_name.as_str(), &buf).unwrap();

    // webm convert to mp3
    let result = Command::new("ffmpeg")
        .args([
            "-i",
            source_name.as_str(),
            "-vn",
            "-ab",
            "128k",
            "-ar",
            "44100",
            "-y",
            target_name.as_str(),
        ])
        .output();

    if result.is_err() {
        let err = result.err();
        eprintln!("ffmpeg error => {err:#?}");
    } else {
        let result = result.unwrap();
        println!("ffmpeg command result => {result:#?}");
    }
}

#[get("/ws")]
async fn ws_handler(
    req: HttpRequest,
    stream: web::Payload,
) -> Result<HttpResponse, actix_web::Error> {
    // let resp = ws::start(MicrophoneWs::new(), &req, stream);
    //
    // println!("{resp:#?}");
    //
    // resp
    let req_protocols = req.headers().get("sec-websocket-protocol");

    if req_protocols.is_none() {
        eprintln!("req protocol not exists!");

        return Err(actix_web::error::ErrorBadRequest(
            "Protocol header not exists!",
        ));
    }

    let req_protocols = req_protocols.unwrap();
    let req_protocols = req_protocols.to_str();

    if req_protocols.is_err() {
        eprintln!("{:#?}", req_protocols.err());

        return Err(actix_web::error::ErrorInternalServerError(
            "Internal Server Error!",
        ));
    }

    let req_protocols = req_protocols.unwrap();
    let req_protocols: Vec<&str> = req_protocols.split(",").collect();

    for proto in req_protocols.iter() {
        if !PROTOCOLS.contains(&proto.trim()) {
            eprintln!("{proto:?} is invalid protocol");

            return Err(actix_web::error::ErrorBadRequest("Invalid protocols!"));
        }
    }

    ws::WsResponseBuilder::new(MicrophoneWs::new(), &req, stream)
        .frame_size(10240000)
        .protocols(PROTOCOLS)
        .start()
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    HttpServer::new(|| {
        App::new()
            .service(heartbeat)
            .service(save_file)
            .service(ws_handler)
            .wrap(
                Cors::default()
                    .allow_any_origin()
                    .allow_any_method()
                    .allow_any_header(),
            )
    })
    .workers(1)
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}

#[post("/save")]
async fn save_file(data: actix_web::web::Json<Payload>) -> impl Responder {
    let audio_data = data.audio_str.split(",").collect::<Vec<&str>>()[1];
    let audio_data = BASE64_STANDARD.decode(audio_data);

    match audio_data {
        Ok(data) => {
            // 파일 저장
            let filename = chrono::Utc::now().timestamp_millis();

            std::fs::write(format!("{}.mp3", filename), &data).unwrap();
        }
        Err(_) => return actix_web::HttpResponse::BadRequest().body("Invalid audio data"),
    };

    actix_web::HttpResponse::Ok().body("File saved successfully")
}

#[get("/")]
async fn heartbeat() -> impl Responder {
    println!("echo!!!");

    actix_web::HttpResponse::Ok().body("ok")
}

use crate::paths;
use crate::proto::{Request, Response};

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

pub struct Client {
    reader: BufReader<UnixStream>,
    writer: UnixStream,
}

impl Client {
    pub fn connect() -> std::io::Result<Client> {
        let stream = UnixStream::connect(paths::socket_path())?;
        let writer = stream.try_clone()?;
        Ok(Client {
            reader: BufReader::new(stream),
            writer,
        })
    }

    /// Connect, launching the Cucina app first if it isn't already up. This is
    /// what makes `cucina up api` work from a cold start in an agent session.
    pub fn connect_or_launch() -> Result<Client, String> {
        if let Ok(c) = Client::connect() {
            return Ok(c);
        }
        let launched = Command::new("/usr/bin/open")
            .args(["-ga", "Cucina"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !launched {
            return Err(
                "Cucina isn't running and couldn't be launched. Open the Cucina app and try again."
                    .into(),
            );
        }
        let deadline = Instant::now() + Duration::from_secs(20);
        while Instant::now() < deadline {
            thread::sleep(Duration::from_millis(200));
            if let Ok(c) = Client::connect() {
                return Ok(c);
            }
        }
        Err("Timed out waiting for Cucina to start.".into())
    }

    pub fn call(&mut self, req: &Request) -> Result<Response, String> {
        let mut body = serde_json::to_vec(req).map_err(|e| e.to_string())?;
        body.push(b'\n');
        self.writer.write_all(&body).map_err(|e| e.to_string())?;
        self.writer.flush().map_err(|e| e.to_string())?;

        let mut line = String::new();
        self.reader
            .read_line(&mut line)
            .map_err(|e| e.to_string())?;
        if line.trim().is_empty() {
            return Err("Cucina closed the connection.".into());
        }
        serde_json::from_str::<Response>(&line).map_err(|e| e.to_string())
    }

    /// Call and unwrap the app-level error, so callers deal with one Result.
    pub fn request(&mut self, req: &Request) -> Result<Response, String> {
        let res = self.call(req)?;
        if res.ok {
            Ok(res)
        } else {
            Err(res.error.unwrap_or_else(|| "Unknown error.".into()))
        }
    }
}

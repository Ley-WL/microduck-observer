//! DuckLink/1 wire compatibility with the Android codec and existing identity file.
use aes_gcm::{
    aead::{Aead, Payload},
    Aes256Gcm, KeyInit, Nonce,
};
use anyhow::{bail, ensure, Result};
use hmac::{Hmac, Mac};
use rand::RngCore;
use serde_json::{json, Value};
use sha2::Sha256;
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

pub const SERVICE: &str = "5d6b1000-6c69-4e6b-9f21-4475636b4c6b";
pub const RX: &str = "5d6b1001-6c69-4e6b-9f21-4475636b4c6b";
pub const TX: &str = "5d6b1002-6c69-4e6b-9f21-4475636b4c6b";
const MAX_FRAME: usize = 8192;
fn valid_hex(s: &str, size: usize) -> bool {
    s.len() == size
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn random_hex(bytes: usize) -> String {
    let mut v = vec![0; bytes];
    rand::thread_rng().fill_bytes(&mut v);
    hex::encode(v)
}
pub fn derive(key: &[u8], context: &str) -> [u8; 32] {
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(key).expect("HMAC key");
    mac.update(context.as_bytes());
    mac.finalize().into_bytes().into()
}
pub struct Identity {
    path: PathBuf,
    pub data: Value,
}
impl Identity {
    pub fn load(path: PathBuf) -> Result<Self> {
        // Upgrades must never rotate a provisioned identity or silently create another one.
        let data: Value = serde_json::from_slice(&std::fs::read(&path)?)?;
        ensure!(
            valid_hex(data["robotId"].as_str().unwrap_or(""), 16)
                && valid_hex(data["setupCode"].as_str().unwrap_or(""), 32),
            "Invalid identity"
        );
        if !data["owner"].is_null() {
            ensure!(
                valid_hex(data["owner"]["client"].as_str().unwrap_or(""), 32)
                    && valid_hex(data["owner"]["key"].as_str().unwrap_or(""), 64),
                "Invalid owner"
            );
        }
        Ok(Self { path, data })
    }
    pub fn robot_id(&self) -> &str {
        self.data["robotId"].as_str().unwrap()
    }
    pub fn owner_key(&self, client: &str) -> Result<[u8; 32]> {
        ensure!(valid_hex(client, 32), "INVALID_HELLO");
        if !self.data["owner"].is_null() {
            ensure!(
                self.data["owner"]["client"] == client,
                "OWNED_BY_ANOTHER_PHONE"
            );
            Ok(hex::decode(self.data["owner"]["key"].as_str().unwrap())?
                .try_into()
                .unwrap())
        } else {
            Ok(derive(
                &hex::decode(self.data["setupCode"].as_str().unwrap())?,
                &format!("owner|{client}"),
            ))
        }
    }
    fn save(&self, value: &Value) -> Result<()> {
        use std::io::Write;
        let tmp = self.path.with_extension("rust-tmp");
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let result = (|| -> Result<()> {
            let mut file = options.open(&tmp)?;
            file.write_all(&serde_json::to_vec(value)?)?;
            file.sync_all()?;
            std::fs::rename(&tmp, &self.path)?;
            std::fs::File::open(self.path.parent().unwrap())?.sync_all()?;
            Ok(())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
        result
    }
    pub fn bind(&mut self, client: &str, key: &[u8; 32]) -> Result<()> {
        if !self.data["owner"].is_null() {
            ensure!(
                self.data["owner"]["client"] == client,
                "OWNED_BY_ANOTHER_PHONE"
            );
            return Ok(());
        }
        let mut next = self.data.clone();
        next["owner"] = json!({"client":client,"key":hex::encode(key)});
        self.save(&next)?;
        self.data = next;
        Ok(())
    }
    pub fn unbind(&mut self, client: &str) -> Result<()> {
        ensure!(self.data["owner"]["client"] == client, "OWNER_REQUIRED");
        let mut next = self.data.clone();
        next["owner"] = Value::Null;
        self.save(&next)?;
        self.data = next;
        Ok(())
    }
}
#[derive(Default)]
pub struct Framer {
    data: Vec<u8>,
    index: u16,
    started: Option<Instant>,
}
impl Framer {
    pub fn feed(&mut self, chunk: &[u8]) -> Result<Option<Vec<u8>>> {
        ensure!(chunk.len() >= 3 && chunk[2] <= 1, "Invalid BLE fragment");
        let index = u16::from_be_bytes([chunk[0], chunk[1]]);
        if index == 0 {
            self.data.clear();
            self.index = 0;
            self.started = Some(Instant::now());
        }
        if index != self.index
            || self
                .started
                .is_none_or(|t| t.elapsed() > Duration::from_secs(3))
            || self.data.len() + chunk.len() - 3 > MAX_FRAME
        {
            self.data.clear();
            self.started = None;
            bail!("BLE fragment sequence/size/timeout");
        }
        self.data.extend_from_slice(&chunk[3..]);
        self.index += 1;
        if chunk[2] == 1 {
            self.index = 0;
            self.started = None;
            Ok(Some(std::mem::take(&mut self.data)))
        } else {
            Ok(None)
        }
    }
}
pub fn fragments(frame: &[u8], size: usize) -> Result<Vec<Vec<u8>>> {
    ensure!(
        (20..=244).contains(&size) && !frame.is_empty() && frame.len() <= MAX_FRAME,
        "BLE frame size"
    );
    let n = frame.len().div_ceil(size - 3);
    Ok(frame
        .chunks(size - 3)
        .enumerate()
        .map(|(i, c)| {
            let mut v = (i as u16).to_be_bytes().to_vec();
            v.push(u8::from(i + 1 == n));
            v.extend_from_slice(c);
            v
        })
        .collect())
}
pub struct Session {
    pub client: Option<String>,
    key: Option<[u8; 32]>,
    reader: Option<Aes256Gcm>,
    writer: Option<Aes256Gcm>,
    recv: u64,
    send: u64,
    started: Instant,
    pub authenticated: bool,
    pub revoked: bool,
}
impl Default for Session {
    fn default() -> Self {
        Self {
            client: None,
            key: None,
            reader: None,
            writer: None,
            recv: 0,
            send: 0,
            started: Instant::now(),
            authenticated: false,
            revoked: false,
        }
    }
}
impl Session {
    pub fn hello(&mut self, frame: &[u8], identity: &Identity) -> Result<Vec<u8>> {
        ensure!(
            !self.revoked && self.client.is_none() && frame.first() == Some(&0),
            "HELLO_ALREADY_RECEIVED"
        );
        let hello: Value = serde_json::from_slice(&frame[1..])?;
        let client = hello["client"].as_str().unwrap_or("");
        ensure!(
            hello["type"] == "hello" && hello["v"] == 1 && valid_hex(client, 32),
            "INVALID_HELLO"
        );
        let key = identity.owner_key(client)?;
        let nonce = random_hex(32);
        let context = format!("{}|{}", identity.robot_id(), nonce);
        self.reader =
            Some(Aes256Gcm::new_from_slice(&derive(&key, &format!("c2s|{context}"))).unwrap());
        self.writer =
            Some(Aes256Gcm::new_from_slice(&derive(&key, &format!("s2c|{context}"))).unwrap());
        self.client = Some(client.into());
        self.key = Some(key);
        self.started = Instant::now();
        let mut reply = vec![0];
        reply.extend(serde_json::to_vec(&json!({"v":1,"robotId":identity.robot_id(),"nonce":nonce,"bound":!identity.data["owner"].is_null()}))?);
        Ok(reply)
    }
    pub fn decrypt(&mut self, frame: &[u8], identity: &Identity) -> Result<(u64, Value)> {
        ensure!(
            !self.revoked && frame.len() >= 25 && frame[0] == 1,
            "INVALID_ENCRYPTED_FRAME"
        );
        ensure!(
            self.authenticated || self.started.elapsed() <= Duration::from_secs(20),
            "AUTH_TIMEOUT"
        );
        let seq = u64::from_be_bytes(frame[1..9].try_into()?);
        ensure!(
            seq == self.recv.checked_add(1).unwrap_or(0) && seq > 0,
            "REPLAY_OR_SEQUENCE_GAP"
        );
        let mut nonce = [0; 12];
        nonce[3] = 1;
        nonce[4..].copy_from_slice(&frame[1..9]);
        let aad = format!("DuckLink/1|{}", identity.robot_id());
        let plain = self
            .reader
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("AUTH_REQUIRED"))?
            .decrypt(
                Nonce::from_slice(&nonce),
                Payload {
                    msg: &frame[9..],
                    aad: aad.as_bytes(),
                },
            )
            .map_err(|_| anyhow::anyhow!("AUTH_OR_PROTOCOL_FAILED"))?;
        let body: Value = serde_json::from_slice(&plain)?;
        ensure!(body.is_object(), "INVALID_MESSAGE");
        self.recv = seq;
        Ok((seq, body))
    }
    pub fn authenticate(&mut self, body: &Value, identity: &mut Identity) -> Result<()> {
        ensure!(
            !self.authenticated
                && body["type"] == "auth"
                && body["client"].as_str() == self.client.as_deref(),
            "AUTH_REQUIRED"
        );
        identity.bind(self.client.as_ref().unwrap(), self.key.as_ref().unwrap())?;
        self.authenticated = true;
        Ok(())
    }
    pub fn encrypt(&mut self, request: u64, result: Value, identity: &Identity) -> Result<Vec<u8>> {
        self.send = self
            .send
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("sequence exhausted"))?;
        let counter = self.send.to_be_bytes();
        let mut nonce = [0; 12];
        nonce[3] = 1;
        nonce[4..].copy_from_slice(&counter);
        let aad = format!("DuckLink/1|{}", identity.robot_id());
        let msg = serde_json::to_vec(&json!({"request":request,"result":result}))?;
        let encrypted = self
            .writer
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("AUTH_REQUIRED"))?
            .encrypt(
                Nonce::from_slice(&nonce),
                Payload {
                    msg: &msg,
                    aad: aad.as_bytes(),
                },
            )
            .map_err(|_| anyhow::anyhow!("encryption failed"))?;
        let mut frame = vec![1];
        frame.extend(counter);
        frame.extend(encrypted);
        ensure!(frame.len() <= MAX_FRAME, "response too large");
        Ok(frame)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fragments_reject_gaps_and_oversize() {
        for size in [20, 182, 244] {
            let frame = vec![42; 8000];
            let mut f = Framer::default();
            let mut out = None;
            for c in fragments(&frame, size).unwrap() {
                out = f.feed(&c).unwrap();
            }
            assert_eq!(out.unwrap(), frame);
        }
        let mut f = Framer::default();
        let chunks = fragments(&vec![1; 500], 20).unwrap();
        f.feed(&chunks[0]).unwrap();
        assert!(f.feed(&chunks[2]).is_err());
        assert!(fragments(&vec![0; 8193], 20).is_err());
    }
    #[test]
    fn identity_binding_survives_restart_and_rejects_other_phone() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("identity.json");
        std::fs::write(&path,json!({"robotId":"1234567890abcdef","setupCode":"000102030405060708090a0b0c0d0e0f","owner":null}).to_string()).unwrap();
        let mut identity = Identity::load(path.clone()).unwrap();
        let client = "a".repeat(32);
        let key = identity.owner_key(&client).unwrap();
        identity.bind(&client, &key).unwrap();
        let mut reloaded = Identity::load(path).unwrap();
        assert_eq!(reloaded.owner_key(&client).unwrap(), key);
        assert!(reloaded.owner_key(&"b".repeat(32)).is_err());
        assert!(reloaded.unbind(&"b".repeat(32)).is_err());
        reloaded.unbind(&client).unwrap();
        assert!(reloaded.data["owner"].is_null());
        assert!(reloaded.owner_key(&"b".repeat(32)).is_ok());
    }
    #[test]
    fn android_vector_crypto_and_replay() {
        let v: Value =
            serde_json::from_str(include_str!("../tests/fixtures/ble-v1-vector.json")).unwrap();
        let key = hex::decode(v["ownerKey"].as_str().unwrap()).unwrap();
        let context = format!(
            "{}|{}",
            v["robotId"].as_str().unwrap(),
            v["nonce"].as_str().unwrap()
        );
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("identity.json");
        std::fs::write(
            &path,
            json!({"robotId":v["robotId"],"setupCode":v["setupCode"],"owner":null}).to_string(),
        )
        .unwrap();
        let identity = Identity::load(path).unwrap();
        let mut session = Session::default();
        session.reader =
            Some(Aes256Gcm::new_from_slice(&derive(&key, &format!("c2s|{context}"))).unwrap());
        let frame = hex::decode(v["requestHex"].as_str().unwrap()).unwrap();
        let (seq, body) = session.decrypt(&frame, &identity).unwrap();
        assert_eq!(seq, 1);
        assert_eq!(body, v["request"]);
        assert!(session.decrypt(&frame, &identity).is_err());
        frame.iter().enumerate().for_each(|(i, _)| {
            if i > 8 {
                let mut altered = frame.clone();
                altered[i] ^= 1;
                let mut fresh = Session::default();
                fresh.reader = Some(
                    Aes256Gcm::new_from_slice(&derive(&key, &format!("c2s|{context}"))).unwrap(),
                );
                assert!(fresh.decrypt(&altered, &identity).is_err());
            }
        });
    }
}

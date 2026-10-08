//! BlueZ peripheral hosted by the observer process; no Python/HTTP motion bridge.
use crate::{
    ducklink_motion::{wifi, Motion},
    ducklink_protocol::{fragments, Framer, Identity, Session, RX, SERVICE, TX},
    App,
};
use anyhow::{ensure, Result};
use axum::Router;
use bluer::{
    adv::Advertisement,
    gatt::local::{
        Application, Characteristic, CharacteristicNotify, CharacteristicNotifyMethod,
        CharacteristicWrite, CharacteristicWriteMethod, ReqError, Service,
    },
};
use futures::FutureExt;
use serde_json::{json, Value};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tokio::sync::{mpsc, Mutex};
struct Peer {
    address: bluer::Address,
    session: Session,
    framer: Framer,
    last: Instant,
}
struct State {
    identity: Identity,
    peer: Option<Peer>,
    motion: Motion,
    notify: Option<mpsc::Sender<Vec<Vec<u8>>>>,
}
impl State {
    async fn drop_peer(&mut self) {
        self.peer = None;
        self.motion.close().await;
    }
    async fn receive(
        &mut self,
        address: bluer::Address,
        data: Vec<u8>,
        mtu: u16,
    ) -> Result<Option<Vec<Vec<u8>>>> {
        ensure!(
            mtu >= 185 && self.notify.as_ref().is_some_and(|tx| !tx.is_closed()),
            "Subscribe with MTU >=185 before writing"
        );
        if let Some(peer) = &self.peer {
            ensure!(peer.address == address, "Robot connected to another phone");
        } else {
            self.peer = Some(Peer {
                address,
                session: Session::default(),
                framer: Framer::default(),
                last: Instant::now(),
            });
        }
        let peer = self.peer.as_mut().unwrap();
        let Some(frame) = peer.framer.feed(&data)? else {
            return Ok(None);
        };
        let answer = if frame.first() == Some(&0) {
            peer.session.hello(&frame, &self.identity)?
        } else {
            let (seq, body) = peer.session.decrypt(&frame, &self.identity)?;
            let result = if !peer.session.authenticated {
                let result = match self.motion.open() {
                    Ok(mut status) => match peer.session.authenticate(&body, &mut self.identity) {
                        Ok(()) => {
                            status["robotId"] = json!(self.identity.robot_id());
                            status["wifiManagement"] = json!(true);
                            status
                        }
                        Err(e) => {
                            self.motion.close().await;
                            json!({"error":e.to_string()})
                        }
                    },
                    Err(e) => json!({"error":e.to_string()}),
                };
                result
            } else {
                let result: Result<Value> = match body["type"].as_str() {
                    Some("command") => self.motion.command(body).await,
                    Some("calibration") => {
                        let payload = body
                            .get("patch")
                            .map(|patch| json!({"patch":patch,"revision":body["revision"]}));
                        self.motion
                            .request("calibration", payload)
                            .await
                            .map(|value| json!({"calibration":value}))
                    }
                    Some("wifi") => {
                        self.motion.halt(false).await?;
                        wifi(body).await
                    }
                    Some("unbind") => {
                        self.identity
                            .unbind(peer.session.client.as_ref().unwrap())?;
                        self.motion.close().await;
                        peer.session.revoked = true;
                        Ok(json!({"unbound":true}))
                    }
                    _ => Err(anyhow::anyhow!("INVALID_COMMAND")),
                };
                match result {
                    Ok(v) => v,
                    Err(e) => json!({"error":e.to_string()}),
                }
            };
            peer.session.encrypt(seq, result, &self.identity)?
        };
        peer.last = Instant::now();
        Ok(Some(fragments(&answer, usize::from(mtu - 3).min(244))?))
    }
}
pub async fn run(app: App, router: Router, stop: Arc<AtomicBool>) -> Result<()> {
    let identity = Identity::load(
        std::env::var("MICRODUCK_BLE_IDENTITY")
            .unwrap_or_else(|_| "/var/lib/ducklink/identity.json".into())
            .into(),
    )?;
    let name = format!("DuckLink-{}", &identity.robot_id()[10..]);
    let state = Arc::new(Mutex::new(State {
        identity,
        peer: None,
        motion: Motion::new(app.clone(), router),
        notify: None,
    }));
    let session = bluer::Session::new().await?;
    let adapter = session.default_adapter().await?;
    adapter.set_powered(true).await?;
    let write_state = state.clone();
    let notify_state = state.clone();
    let application = Application {
        services: vec![Service {
            uuid: SERVICE.parse()?,
            primary: true,
            characteristics: vec![
                Characteristic {
                    uuid: RX.parse()?,
                    write: Some(CharacteristicWrite {
                        write: true,
                        method: CharacteristicWriteMethod::Fun(Box::new(move |data, req| {
                            let state = write_state.clone();
                            async move {
                                if req.offset != 0 || req.prepare_authorize {
                                    return Err(ReqError::NotSupported);
                                }
                                let mut s = state.lock().await;
                                match s.receive(req.device_address, data, req.mtu).await {
                                    Ok(Some(chunks)) => {
                                        if let Some(tx) = &s.notify {
                                            tx.try_send(chunks)
                                                .map_err(|_| ReqError::InProgress)?;
                                        }
                                        Ok(())
                                    }
                                    Ok(None) => Ok(()),
                                    Err(e) => {
                                        // Do not evict an existing controller for a second peer's writes.
                                        if s.peer
                                            .as_ref()
                                            .is_some_and(|p| p.address == req.device_address)
                                        {
                                            let code = if e.to_string() == "OWNED_BY_ANOTHER_PHONE"
                                            {
                                                "OWNED_BY_ANOTHER_PHONE"
                                            } else {
                                                "AUTH_OR_PROTOCOL_FAILED"
                                            };
                                            let mut frame = vec![0];
                                            frame.extend(
                                                serde_json::to_vec(&json!({"error":code})).unwrap(),
                                            );
                                            if let Some(tx) = &s.notify {
                                                let _ = tx.try_send(
                                                    fragments(
                                                        &frame,
                                                        usize::from(req.mtu.saturating_sub(3))
                                                            .clamp(20, 244),
                                                    )
                                                    .unwrap(),
                                                );
                                            }
                                            s.drop_peer().await;
                                        }
                                        eprintln!("DuckLink BLE request rejected");
                                        Err(ReqError::NotAuthorized)
                                    }
                                }
                            }
                            .boxed()
                        })),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                Characteristic {
                    uuid: TX.parse()?,
                    notify: Some(CharacteristicNotify {
                        notify: true,
                        method: CharacteristicNotifyMethod::Fun(Box::new(move |mut notifier| {
                            let state = notify_state.clone();
                            async move{
                let (tx,mut rx)=mpsc::channel::<Vec<Vec<u8>>>(1);
                {let mut s=state.lock().await;s.drop_peer().await;s.notify=Some(tx);}
                tokio::spawn(async move{
                    loop {
                        tokio::select! {
                            _=notifier.stopped()=>break,
                            chunks=rx.recv()=>{let Some(chunks)=chunks else{break;};let mut failed=false;
                                for chunk in chunks {if notifier.notify(chunk).await.is_err(){failed=true;break;}tokio::time::sleep(Duration::from_millis(8)).await;}if failed{break;}
                            }
                        }
                    }
                    let mut s=state.lock().await;s.notify=None;s.drop_peer().await;
                });
            }.boxed()
                        })),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            ],
            ..Default::default()
        }],
        ..Default::default()
    };
    let _gatt = adapter.serve_gatt_application(application).await?;
    let _adv = adapter
        .advertise(Advertisement {
            service_uuids: [SERVICE.parse()?].into_iter().collect(),
            discoverable: Some(true),
            local_name: Some(name),
            ..Default::default()
        })
        .await?;
    {
        app.telemetry.write().unwrap().ble_health =
            json!({"state":"ready","backend":"rust","protocol":1});
    }
    eprintln!("DuckLink BLE ready in Rust backend");
    let mut timer = tokio::time::interval(Duration::from_millis(80));
    timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let result: Result<()> = async {
        while !stop.load(Ordering::Acquire) {
            timer.tick().await;
            adapter.is_powered().await?;
            let mut s = state.lock().await;
            if let Some(peer) = &s.peer {
                let connected = adapter
                    .device(peer.address)?
                    .is_connected()
                    .await
                    .unwrap_or(false);
                let limit = if peer.session.authenticated { 5 } else { 20 };
                if !connected
                    || peer.last.elapsed() > Duration::from_secs(limit)
                    || peer.session.revoked
                {
                    let address = peer.address;
                    s.drop_peer().await;
                    if connected {
                        let _ = adapter.device(address)?.disconnect().await;
                    }
                }
            }
            s.motion.tick().await;
        }
        Ok(())
    }
    .await;
    state.lock().await.drop_peer().await;
    result
}
pub fn spawn(app: App, router: Router, stop: Arc<AtomicBool>) {
    tokio::spawn(async move {
        while !stop.load(Ordering::Acquire) {
            if let Err(_error) = run(app.clone(), router.clone(), stop.clone()).await {
                app.telemetry.write().unwrap().ble_health =
                    json!({"state":"unavailable","backend":"rust","protocol":1});
                eprintln!("DuckLink BLE unavailable; retrying in 5 seconds");
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }
    });
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::ducklink_protocol::derive;
    use aes_gcm::{
        aead::{Aead, Payload},
        Aes256Gcm, KeyInit, Nonce,
    };
    async fn exchange(s: &mut State, address: bluer::Address, frame: Vec<u8>) -> Result<Vec<u8>> {
        let mut answer = None;
        for chunk in fragments(&frame, 182)? {
            if let Some(reply) = s.receive(address, chunk, 185).await? {
                answer = Some(reply);
            }
        }
        let mut f = Framer::default();
        let mut result = None;
        for chunk in answer.unwrap() {
            result = f.feed(&chunk)?;
        }
        Ok(result.unwrap())
    }
    fn seal(cipher: &Aes256Gcm, seq: u64, body: Value) -> Vec<u8> {
        let mut nonce = [0; 12];
        nonce[3] = 1;
        nonce[4..].copy_from_slice(&seq.to_be_bytes());
        let mut frame = vec![1];
        frame.extend(seq.to_be_bytes());
        frame.extend(
            cipher
                .encrypt(
                    Nonce::from_slice(&nonce),
                    Payload {
                        msg: &serde_json::to_vec(&body).unwrap(),
                        aad: b"DuckLink/1|1234567890abcdef",
                    },
                )
                .unwrap(),
        );
        frame
    }
    fn open(cipher: &Aes256Gcm, frame: Vec<u8>) -> Value {
        let mut nonce = [0; 12];
        nonce[3] = 1;
        nonce[4..].copy_from_slice(&frame[1..9]);
        serde_json::from_slice(
            &cipher
                .decrypt(
                    Nonce::from_slice(&nonce),
                    Payload {
                        msg: &frame[9..],
                        aad: b"DuckLink/1|1234567890abcdef",
                    },
                )
                .unwrap(),
        )
        .unwrap()
    }
    #[tokio::test]
    async fn full_ble_auth_transport_ownership_and_ready_reconnect() {
        let (motion, calls, tmp) = crate::ducklink_motion::tests::fixture();
        let path = tmp.path().join("identity.json");
        std::fs::write(&path,json!({"robotId":"1234567890abcdef","setupCode":"000102030405060708090a0b0c0d0e0f","owner":null}).to_string()).unwrap();
        let (tx, _rx) = mpsc::channel(1);
        let mut s = State {
            identity: Identity::load(path).unwrap(),
            peer: None,
            motion,
            notify: Some(tx),
        };
        let address = bluer::Address::new([1, 2, 3, 4, 5, 6]);
        let client = "a".repeat(32);
        let hello = [
            vec![0],
            serde_json::to_vec(&json!({"v":1,"type":"hello","client":client})).unwrap(),
        ]
        .concat();
        let greeting = exchange(&mut s, address, hello.clone()).await.unwrap();
        let greeting: Value = serde_json::from_slice(&greeting[1..]).unwrap();
        let key = derive(
            &hex::decode("000102030405060708090a0b0c0d0e0f").unwrap(),
            &format!("owner|{client}"),
        );
        let context = format!("1234567890abcdef|{}", greeting["nonce"].as_str().unwrap());
        let writer = Aes256Gcm::new_from_slice(&derive(&key, &format!("c2s|{context}"))).unwrap();
        let reader = Aes256Gcm::new_from_slice(&derive(&key, &format!("s2c|{context}"))).unwrap();
        let reply = exchange(
            &mut s,
            address,
            seal(
                &writer,
                1,
                json!({"type":"command","seq":1,"action":"stand"}),
            ),
        )
        .await
        .unwrap();
        assert_eq!(open(&reader, reply)["result"]["error"], "AUTH_REQUIRED");
        assert!(calls.lock().unwrap().is_empty());
        let reply = exchange(
            &mut s,
            address,
            seal(&writer, 2, json!({"type":"auth","client":client})),
        )
        .await
        .unwrap();
        let reply = open(&reader, reply);
        assert_eq!(reply["result"]["stopped"], false);
        assert!(s.peer.as_ref().unwrap().session.authenticated);
        assert!(calls.lock().unwrap().is_empty());
        assert!(s
            .receive(
                bluer::Address::new([9; 6]),
                fragments(&hello, 182).unwrap()[0].clone(),
                185
            )
            .await
            .is_err());
        assert!(s.motion.leased);
        let reply = exchange(
            &mut s,
            address,
            seal(
                &writer,
                3,
                json!({"type":"command","seq":1,"action":"drive","forward":1,"turn":0,"speed":0.5}),
            ),
        )
        .await
        .unwrap();
        assert_eq!(open(&reader, reply)["result"]["mode"], "idle");
        assert!(calls.lock().unwrap().is_empty());
        s.drop_peer().await;
        assert!(!s.motion.leased);
        let status = s.motion.open().unwrap();
        assert_eq!(status["stopped"], false);
        assert_eq!(s.identity.owner_key(&client).unwrap(), key);
        assert!(calls.lock().unwrap().is_empty());
    }
}

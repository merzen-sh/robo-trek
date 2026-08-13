use crossbeam_channel::Receiver;
use serenity::{
    builder::{CreateAttachment, CreateMessage},
    http::Http,
    model::id::ChannelId,
};
use std::sync::Arc;

use crate::{db, render};

pub fn spawn(release_rx: Receiver<String>, http: Arc<Http>, channel_id: ChannelId, db: db::Db) {
    tokio::task::spawn_blocking(move || {
        let rt = tokio::runtime::Handle::current();
        while let Ok(version) = release_rx.recv() {
            match render::release_card(&version) {
                Ok(png) => {
                    if let Err(e) = rt.block_on(db.put_release(&version, &png)) {
                        eprintln!("failed to cache release card: {e}");
                    }
                    let attachment = CreateAttachment::bytes(png, "release.png");
                    let msg = CreateMessage::new()
                        .content(format!("Version {version} is out!"))
                        .add_file(attachment);
                    if let Err(e) = rt.block_on(channel_id.send_message(&http, msg)) {
                        eprintln!("failed to send release to discord: {e}");
                    }
                }
                Err(e) => eprintln!("failed to render release card: {e}"),
            }
        }
    });
}

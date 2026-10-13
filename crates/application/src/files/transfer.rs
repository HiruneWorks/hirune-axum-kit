use super::{FileSettings, TransferStream};
use crate::{ApplicationError, ApplicationResult};
use futures_util::{StreamExt, stream};
use std::{
    io,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    io::AsyncWriteExt,
    sync::{OwnedSemaphorePermit, Semaphore},
    time::Instant,
};
use tracing::Instrument;

pub(super) fn acquire_transfer(
    permits: &Arc<Semaphore>,
) -> ApplicationResult<OwnedSemaphorePermit> {
    permits
        .clone()
        .try_acquire_owned()
        .map_err(|_| ApplicationError::IntegrationUnavailable)
}

pub(super) async fn spool_original(
    mut body: TransferStream,
    expected_bytes: u64,
    settings: &FileSettings,
) -> ApplicationResult<(tempfile::NamedTempFile, String)> {
    let temporary = tempfile::Builder::new()
        .prefix("hirune-upload-")
        .tempfile_in(&settings.temporary_directory)
        .map_err(ApplicationError::internal)?;
    let mut file =
        tokio::fs::File::from_std(temporary.reopen().map_err(ApplicationError::internal)?);
    let mut received_bytes = 0_u64;
    let mut hasher = blake3::Hasher::new();
    let deadline = Instant::now() + Duration::from_secs(settings.transfer_seconds);
    loop {
        let next = tokio::time::timeout_at(
            deadline.min(Instant::now() + Duration::from_secs(settings.idle_seconds)),
            body.next(),
        )
        .await
        .map_err(|_| ApplicationError::TransferTimeout)?;
        let Some(chunk) = next else { break };
        let chunk = chunk.map_err(|_| ApplicationError::InvalidInput("upload body interrupted"))?;
        received_bytes = received_bytes
            .checked_add(chunk.len() as u64)
            .ok_or(ApplicationError::FileTooLarge)?;
        if received_bytes > expected_bytes || received_bytes > settings.max_file_bytes {
            return Err(ApplicationError::FileTooLarge);
        }
        hasher.update(&chunk);
        file.write_all(&chunk)
            .await
            .map_err(ApplicationError::internal)?;
    }
    if received_bytes != expected_bytes {
        return Err(ApplicationError::InvalidInput(
            "file length does not match reservation",
        ));
    }
    file.flush().await.map_err(ApplicationError::internal)?;
    drop(file);
    Ok((temporary, hasher.finalize().to_hex().to_string()))
}

pub(super) fn bounded_download(
    mut body: TransferStream,
    permit: OwnedSemaphorePermit,
    byte_length: u64,
    settings: &FileSettings,
) -> TransferStream {
    let idle = Duration::from_secs(settings.idle_seconds);
    let deadline = Instant::now() + Duration::from_secs(settings.transfer_seconds);
    let cancellation = tokio_util::sync::CancellationToken::new();
    let shared_permit = Arc::new(Mutex::new(Some(permit)));
    let guard = DownloadLease {
        permit: shared_permit.clone(),
        cancellation: cancellation.clone(),
    };
    let (sender, receiver) = tokio::sync::mpsc::channel(2);
    let (outcome_sender, outcome_receiver) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        let forwarding = async {
            let mut remaining = byte_length;
            while remaining > 0 {
                let chunk = tokio::time::timeout(idle, body.next()).await
                    .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "original read timed out"))?
                    .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "original ended early"))?
                    .map_err(|_| io::Error::other("original read failed"))?;
                if chunk.is_empty() || chunk.len() as u64 > remaining {
                    return Err(io::Error::new(io::ErrorKind::InvalidData, "original length mismatch"));
                }
                remaining -= chunk.len() as u64;
                tokio::time::timeout(idle, sender.send(chunk)).await
                    .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "download consumer stalled"))?
                    .map_err(|_| io::Error::new(io::ErrorKind::Interrupted, "download closed"))?;
            }
            Ok(())
        };
        let outcome = tokio::select! {
            _ = cancellation.cancelled() => Err(io::Error::new(io::ErrorKind::Interrupted, "download closed")),
            outcome = tokio::time::timeout_at(deadline, forwarding) => outcome.unwrap_or_else(|_| Err(io::Error::new(io::ErrorKind::TimedOut, "download deadline elapsed"))),
        };
        drop(body);
        drop(sender);
        let _ = outcome_sender.send(outcome);
        // The lease ends on consumption/drop or the independent deadline, even if unpolled.
        tokio::select! { _ = cancellation.cancelled() => {}, _ = tokio::time::sleep_until(deadline) => {} }
        shared_permit.lock().expect("transfer lease lock").take();
    }.instrument(tracing::Span::current()));
    stream::unfold(
        (receiver, Some(outcome_receiver), guard),
        |(mut receiver, mut outcome, guard)| async move {
            if let Some(chunk) = receiver.recv().await {
                return Some((Ok(chunk), (receiver, outcome, guard)));
            }
            let outcome = outcome
                .take()?
                .await
                .unwrap_or_else(|_| Err(io::Error::other("download task interrupted")));
            outcome
                .err()
                .map(|error| (Err(error), (receiver, None, guard)))
        },
    )
    .boxed()
}

struct DownloadLease {
    permit: Arc<Mutex<Option<OwnedSemaphorePermit>>>,
    cancellation: tokio_util::sync::CancellationToken,
}
impl Drop for DownloadLease {
    fn drop(&mut self) {
        self.permit.lock().expect("transfer lease lock").take();
        self.cancellation.cancel();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn an_unpolled_response_releases_its_transfer_slot_at_the_deadline() {
        let permits = Arc::new(Semaphore::new(1));
        let settings = FileSettings {
            transfer_seconds: 30,
            idle_seconds: 30,
            ..Default::default()
        };
        let mut response = bounded_download(
            stream::pending().boxed(),
            acquire_transfer(&permits).unwrap(),
            4,
            &settings,
        );
        tokio::task::yield_now().await;
        assert_eq!(permits.available_permits(), 0);
        tokio::time::advance(Duration::from_secs(31)).await;
        tokio::task::yield_now().await;
        assert_eq!(permits.available_permits(), 1);
        assert_eq!(
            response.next().await.unwrap().unwrap_err().kind(),
            io::ErrorKind::TimedOut
        );
    }
}

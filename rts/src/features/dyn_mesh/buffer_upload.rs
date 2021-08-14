use crossbeam_channel::{Receiver, Sender};
use rafx::{
    api::{
        extra::upload::*, RafxBuffer, RafxDeviceContext, RafxError, RafxQueue, RafxResourceType,
        RafxResult,
    },
    assets::buffer_upload::enqueue_load_buffer,
};
use std::sync::atomic::{AtomicU64, Ordering};

pub enum BufferUploadResult {
    UploadError(BufferUploadId),
    UploadComplete(BufferUploadId, RafxBuffer),
    UploadDrop(BufferUploadId),
}

#[derive(Clone, Debug)]
pub struct BufferUploadId {
    id: u64,
}

pub struct BufferUploaderConfig {
    pub max_bytes_per_transfer: usize,
    pub max_concurrent_transfers: usize,
    pub max_new_transfers_in_single_frame: usize,
}

enum UploadOpResult {
    UploadError(BufferUploadId, Sender<BufferUploadResult>),
    UploadComplete(BufferUploadId, Sender<BufferUploadResult>, RafxBuffer),
    UploadDrop(BufferUploadId, Sender<BufferUploadResult>),
}

struct UploadOp {
    upload_id: BufferUploadId,
    result_tx: Option<Sender<BufferUploadResult>>,
    sender: Option<Sender<UploadOpResult>>,
}

impl UploadOp {
    pub fn new(
        upload_id: BufferUploadId,
        result_tx: Sender<BufferUploadResult>,
        sender: Sender<UploadOpResult>,
    ) -> Self {
        Self {
            upload_id,
            result_tx: Some(result_tx),
            sender: Some(sender),
        }
    }

    pub fn complete(mut self, buffer: RafxBuffer) {
        let _ = self
            .sender
            .as_ref()
            .unwrap()
            .send(UploadOpResult::UploadComplete(
                self.upload_id.clone(),
                self.result_tx.take().unwrap(),
                buffer,
            ));
        self.sender = None;
    }

    pub fn error(mut self) {
        let _ = self
            .sender
            .as_ref()
            .unwrap()
            .send(UploadOpResult::UploadError(
                self.upload_id.clone(),
                self.result_tx.take().unwrap(),
            ));
        self.sender = None;
    }
}

impl Drop for UploadOp {
    fn drop(&mut self) {
        if let Some(ref sender) = self.sender {
            let _ = sender.send(UploadOpResult::UploadDrop(
                self.upload_id.clone(),
                self.result_tx.take().unwrap(),
            ));
        }
    }
}

struct PendingUpload {
    pub upload_op: UploadOp,
    pub resource_type: RafxResourceType,
    pub data: Vec<u8>,
}

struct InFlightUpload {
    upload_op: UploadOp,
    buffer: RafxBuffer,
}

enum InProgressTransferPollResult {
    Pending,
    Complete,
    Error,
    Destroyed,
}

struct InProgressTransferInner {
    in_flight_uploads: Vec<InFlightUpload>,
    transfer: RafxTransferUpload,
}

struct InProgressTransferDebugInfo {
    transfer_id: usize,
    start_time: rafx::base::Instant,
    size: u64,
    buffer_count: usize,
}

struct InProgressTransfer {
    inner: Option<InProgressTransferInner>,
    debug_info: InProgressTransferDebugInfo,
}

impl InProgressTransfer {
    pub fn new(
        in_flight_uploads: Vec<InFlightUpload>,
        transfer: RafxTransferUpload,
        debug_info: InProgressTransferDebugInfo,
    ) -> Self {
        let inner = InProgressTransferInner {
            in_flight_uploads,
            transfer,
        };

        InProgressTransfer {
            inner: Some(inner),
            debug_info,
        }
    }

    pub fn poll(&mut self) -> InProgressTransferPollResult {
        loop {
            if let Some(mut inner) = self.take_inner() {
                match inner.transfer.state() {
                    Ok(state) => match state {
                        RafxTransferUploadState::Writable => {
                            inner.transfer.submit_transfer().unwrap();
                            self.inner = Some(inner);
                        }
                        RafxTransferUploadState::SentToTransferQueue => {
                            self.inner = Some(inner);
                            break InProgressTransferPollResult::Pending;
                        }
                        RafxTransferUploadState::PendingSubmitDstQueue => {
                            inner.transfer.submit_dst().unwrap();
                            self.inner = Some(inner);
                        }
                        RafxTransferUploadState::SentToDstQueue => {
                            self.inner = Some(inner);
                            break InProgressTransferPollResult::Pending;
                        }
                        RafxTransferUploadState::Complete => {
                            for upload in inner.in_flight_uploads {
                                let buffer = upload.buffer;
                                upload.upload_op.complete(buffer);
                            }

                            break InProgressTransferPollResult::Complete;
                        }
                    },
                    Err(_err) => {
                        for upload in inner.in_flight_uploads {
                            upload.upload_op.error();
                            // Buffer is dropped here
                        }

                        break InProgressTransferPollResult::Error;
                    }
                }
            } else {
                break InProgressTransferPollResult::Destroyed;
            }
        }
    }

    fn take_inner(&mut self) -> Option<InProgressTransferInner> {
        let mut inner = None;
        std::mem::swap(&mut self.inner, &mut inner);
        inner
    }
}

impl Drop for InProgressTransfer {
    fn drop(&mut self) {
        if let Some(mut inner) = self.take_inner() {
            inner.in_flight_uploads.clear();
        }
    }
}

struct UploadQueue {
    device_context: RafxDeviceContext,
    config: BufferUploaderConfig,

    pending_tx: Sender<PendingUpload>,
    pending_rx: Receiver<PendingUpload>,

    next_upload: Option<PendingUpload>,

    transfers_in_progress: Vec<InProgressTransfer>,

    graphics_queue: RafxQueue,
    transfer_queue: RafxQueue,

    next_transfer_id: usize,
}

impl Drop for UploadQueue {
    fn drop(&mut self) {
        log::info!("Cleaning up buffer upload manager queue");

        self.transfer_queue.wait_for_queue_idle().unwrap();
        self.graphics_queue.wait_for_queue_idle().unwrap();

        log::info!("Dropping buffer upload manager queue");
    }
}

impl UploadQueue {
    pub fn new(
        device_context: &RafxDeviceContext,
        config: BufferUploaderConfig,
        graphics_queue: RafxQueue,
        transfer_queue: RafxQueue,
    ) -> Self {
        let (pending_tx, pending_rx) = crossbeam_channel::unbounded();

        UploadQueue {
            device_context: device_context.clone(),
            config,
            pending_tx,
            pending_rx,
            next_upload: None,
            transfers_in_progress: Default::default(),
            next_transfer_id: 1,
            graphics_queue,
            transfer_queue,
        }
    }

    pub fn pending_tx(&self) -> &Sender<PendingUpload> {
        &self.pending_tx
    }

    pub fn update(&mut self) -> RafxResult<()> {
        self.start_new_transfers()?;
        self.update_existing_transfers();
        Ok(())
    }

    fn start_new_transfers(&mut self) -> RafxResult<()> {
        for _ in 0..self.config.max_new_transfers_in_single_frame {
            if self.pending_rx.is_empty() && self.next_upload.is_none() {
                return Ok(());
            }

            if self.transfers_in_progress.len() >= self.config.max_concurrent_transfers {
                log::trace!(
                    "Max number of transfers already in progress. Waiting to start a new one"
                );
                return Ok(());
            }

            if !self.start_new_transfer()? {
                return Ok(());
            }
        }

        Ok(())
    }

    fn start_new_transfer(&mut self) -> RafxResult<bool> {
        let mut transfer = RafxTransferUpload::new(
            &self.device_context,
            &self.transfer_queue,
            &self.graphics_queue,
            self.config.max_bytes_per_transfer as u64,
        )?;

        let in_flight_uploads = self.start_new_uploads(&mut transfer)?;

        if !in_flight_uploads.is_empty() {
            let transfer_id = self.next_transfer_id;
            self.next_transfer_id += 1;

            log::debug!(
                "Submitting {} byte transfer with {} buffers, TransferId = {}",
                transfer.bytes_written(),
                in_flight_uploads.len(),
                transfer_id
            );

            transfer.submit_transfer()?;

            let debug_info = InProgressTransferDebugInfo {
                transfer_id,
                buffer_count: in_flight_uploads.len(),
                size: transfer.bytes_written(),
                start_time: rafx::base::Instant::now(),
            };

            self.transfers_in_progress.push(InProgressTransfer::new(
                in_flight_uploads,
                transfer,
                debug_info,
            ));

            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn start_new_uploads(
        &mut self,
        transfer: &mut RafxTransferUpload,
    ) -> RafxResult<Vec<InFlightUpload>> {
        let mut in_flight_uploads = vec![];

        self.next_upload = if let Some(next_upload) = self.next_upload.take() {
            self.try_enqueue_upload(transfer, next_upload, &mut in_flight_uploads)?
        } else {
            None
        };

        if let Some(next_upload) = &self.next_upload {
            log::error!(
                "Buffer of {} bytes has repeatedly exceeded the available room in the transfer buffer. ({} of {} bytes free)",
                next_upload.data.len(),
                transfer.bytes_free(),
                transfer.buffer_size()
            );
            return Ok(vec![]);
        }

        let rx = self.pending_rx.clone();
        for pending_upload in rx.try_iter() {
            self.next_upload =
                self.try_enqueue_upload(transfer, pending_upload, &mut in_flight_uploads)?;

            if let Some(next_upload) = &self.next_upload {
                log::debug!(
                    "Buffer of {} bytes exceeds the available room in the transfer buffer. ({} of {} bytes free)",
                    next_upload.data.len(),
                    transfer.bytes_free(),
                    transfer.buffer_size(),
                );
                break;
            }
        }

        Ok(in_flight_uploads)
    }

    fn try_enqueue_upload(
        &mut self,
        transfer: &mut RafxTransferUpload,
        pending: PendingUpload,
        in_flight_uploads: &mut Vec<InFlightUpload>,
    ) -> RafxResult<Option<PendingUpload>> {
        let result = enqueue_load_buffer(
            &self.device_context,
            transfer,
            pending.resource_type,
            &pending.data,
        );

        match result {
            Ok(buffer) => {
                in_flight_uploads.push(InFlightUpload {
                    buffer,
                    upload_op: pending.upload_op,
                });
                Ok(None)
            }
            Err(RafxUploadError::Other(e)) => Err(e),
            Err(RafxUploadError::BufferFull) => Ok(Some(pending)),
        }
    }

    fn update_existing_transfers(&mut self) {
        // iterate backwards so we can use swap_remove
        for i in (0..self.transfers_in_progress.len()).rev() {
            let result = self.transfers_in_progress[i].poll();
            match result {
                InProgressTransferPollResult::Pending => {
                    // do nothing
                }
                InProgressTransferPollResult::Complete => {
                    let debug_info = &self.transfers_in_progress[i].debug_info;
                    log::debug!(
                        "Completed {} byte transfer with {} buffers in {} ms, TransferId = {}",
                        debug_info.size,
                        debug_info.buffer_count,
                        debug_info.start_time.elapsed().as_secs_f32(),
                        debug_info.transfer_id
                    );

                    self.transfers_in_progress.swap_remove(i);
                }
                InProgressTransferPollResult::Error => {
                    let debug_info = &self.transfers_in_progress[i].debug_info;
                    log::error!(
                        "Failed {} byte transfer with {} buffers in {} ms, TransferId = {}",
                        debug_info.size,
                        debug_info.buffer_count,
                        debug_info.start_time.elapsed().as_secs_f32(),
                        debug_info.transfer_id
                    );

                    self.transfers_in_progress.swap_remove(i);
                }
                InProgressTransferPollResult::Destroyed => {
                    // not expected - this only occurs if polling the upload when it is already in a complete or error state
                    unreachable!();
                }
            }
        }
    }
}

pub struct BufferUploader {
    upload_queue: UploadQueue,
    current_id: AtomicU64,

    result_tx: Sender<UploadOpResult>,
    result_rx: Receiver<UploadOpResult>,
}

impl BufferUploader {
    pub fn new(
        device_context: &RafxDeviceContext,
        upload_queue_config: BufferUploaderConfig,
        graphics_queue: RafxQueue,
        transfer_queue: RafxQueue,
    ) -> Self {
        let (result_tx, result_rx) = crossbeam_channel::unbounded();

        BufferUploader {
            upload_queue: UploadQueue::new(
                device_context,
                upload_queue_config,
                graphics_queue,
                transfer_queue,
            ),
            current_id: AtomicU64::new(0),
            result_rx,
            result_tx,
        }
    }

    pub fn update(&mut self) -> RafxResult<()> {
        self.upload_queue.update()?;

        let results: Vec<_> = self.result_rx.try_iter().collect();
        for result in results {
            match result {
                UploadOpResult::UploadComplete(upload_id, result_tx, buffer) => {
                    log::trace!("Uploading buffer {:?} complete", upload_id);
                    let _res =
                        result_tx.send(BufferUploadResult::UploadComplete(upload_id, buffer));
                }
                UploadOpResult::UploadError(upload_id, result_tx) => {
                    log::trace!("Uploading buffer {:?} failed", upload_id);
                    let _res = result_tx.send(BufferUploadResult::UploadError(upload_id));
                }
                UploadOpResult::UploadDrop(upload_id, result_tx) => {
                    log::trace!("Uploading buffer {:?} cancelled", upload_id);
                    let _res = result_tx.send(BufferUploadResult::UploadDrop(upload_id));
                }
            }
        }
        Ok(())
    }

    pub fn upload_buffer(
        &self,
        resource_type: RafxResourceType,
        data: Vec<u8>,
        result_tx: Sender<BufferUploadResult>,
    ) -> RafxResult<BufferUploadId> {
        assert!(!data.is_empty());
        let upload_id = BufferUploadId {
            id: self.current_id.fetch_add(1, Ordering::Relaxed),
        };
        let result = self.upload_queue.pending_tx().send(PendingUpload {
            upload_op: UploadOp::new(upload_id.clone(), result_tx, self.result_tx.clone()),
            resource_type,
            data,
        });
        if result.is_err() {
            let error = format!("Could not enqueue buffer upload");
            log::error!("{}", error);
            Err(RafxError::StringError(error))
        } else {
            Ok(upload_id)
        }
    }
}

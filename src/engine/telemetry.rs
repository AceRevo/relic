use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationStatus {
    Idle,
    Scanning,
    Streaming,
    Verifying,
    Completed,
    Failed,
}

#[allow(dead_code)]
#[derive(Clone)]
pub struct StreamTelemetry {
    pub total_bytes: Arc<AtomicU64>,
    pub processed_bytes: Arc<AtomicU64>,
    pub active_chunks: Arc<AtomicU64>,
    pub sparse_chunks: Arc<AtomicU64>,
    pub error_count: Arc<AtomicU64>,
    pub is_running: Arc<AtomicBool>,
    start_time: Arc<Mutex<Option<Instant>>>,
    status: Arc<Mutex<OperationStatus>>,
    speed_mb_s: Arc<Mutex<f64>>,
}

#[allow(dead_code)]
impl StreamTelemetry {
    pub fn new(total_bytes: u64) -> Self {
        Self {
            total_bytes: Arc::new(AtomicU64::new(total_bytes)),
            processed_bytes: Arc::new(AtomicU64::new(0)),
            active_chunks: Arc::new(AtomicU64::new(0)),
            sparse_chunks: Arc::new(AtomicU64::new(0)),
            error_count: Arc::new(AtomicU64::new(0)),
            is_running: Arc::new(AtomicBool::new(false)),
            start_time: Arc::new(Mutex::new(None)),
            status: Arc::new(Mutex::new(OperationStatus::Idle)),
            speed_mb_s: Arc::new(Mutex::new(0.0)),
        }
    }

    pub fn start(&self, total_bytes: u64, status: OperationStatus) {
        self.total_bytes.store(total_bytes, Ordering::SeqCst);
        self.processed_bytes.store(0, Ordering::SeqCst);
        self.active_chunks.store(0, Ordering::SeqCst);
        self.sparse_chunks.store(0, Ordering::SeqCst);
        self.error_count.store(0, Ordering::SeqCst);
        self.is_running.store(true, Ordering::SeqCst);

        let mut st = self.start_time.lock().unwrap();
        *st = Some(Instant::now());

        let mut s = self.status.lock().unwrap();
        *s = status;

        let mut sp = self.speed_mb_s.lock().unwrap();
        *sp = 0.0;
    }

    pub fn update(&self, bytes: u64, is_sparse: bool) {
        self.processed_bytes.fetch_add(bytes, Ordering::SeqCst);
        if is_sparse {
            self.sparse_chunks.fetch_add(1, Ordering::SeqCst);
        } else {
            self.active_chunks.fetch_add(1, Ordering::SeqCst);
        }

        let st = self.start_time.lock().unwrap();
        if let Some(start) = *st {
            let elapsed = start.elapsed().as_secs_f64();
            if elapsed > 0.1 {
                let processed = self.processed_bytes.load(Ordering::SeqCst) as f64;
                let mb_s = (processed / (1024.0 * 1024.0)) / elapsed;
                let mut sp = self.speed_mb_s.lock().unwrap();
                *sp = mb_s;
            }
        }
    }

    pub fn finish(&self, success: bool) {
        self.is_running.store(false, Ordering::SeqCst);
        let mut s = self.status.lock().unwrap();
        *s = if success {
            OperationStatus::Completed
        } else {
            OperationStatus::Failed
        };
    }

    pub fn percent(&self) -> u16 {
        let total = self.total_bytes.load(Ordering::SeqCst);
        if total == 0 {
            return 0;
        }
        let processed = self.processed_bytes.load(Ordering::SeqCst);
        let p = (processed as f64 / total as f64 * 100.0) as u16;
        p.min(100)
    }

    pub fn speed_mb_s(&self) -> f64 {
        *self.speed_mb_s.lock().unwrap()
    }

    pub fn status(&self) -> OperationStatus {
        *self.status.lock().unwrap()
    }

    pub fn eta_seconds(&self) -> Option<u64> {
        let speed = self.speed_mb_s();
        if speed <= 0.01 {
            return None;
        }
        let total = self.total_bytes.load(Ordering::SeqCst);
        let processed = self.processed_bytes.load(Ordering::SeqCst);
        if total <= processed {
            return Some(0);
        }
        let remaining_mb = (total - processed) as f64 / (1024.0 * 1024.0);
        Some((remaining_mb / speed) as u64)
    }
}

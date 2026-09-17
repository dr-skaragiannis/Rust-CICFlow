use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct BulkTracker {
    pub duration: i64,          // microseconds
    pub packet_count: u64,
    pub size_total: u64,
    pub state_count: u64,
    pub packet_count_helper: u64,
    pub start_helper: i64,
    pub size_helper: u64,
    pub last_bulk_ts: i64,
}

impl BulkTracker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn update(&mut self, timestamp_us: i64, payload_bytes: u64, ts_of_last_bulk_in_other: i64) {
        if ts_of_last_bulk_in_other > self.start_helper {
            self.start_helper = 0;
        }

        if payload_bytes == 0 {
            return;
        }

        if self.start_helper == 0 {
            self.start_helper = timestamp_us;
            self.packet_count_helper = 1;
            self.size_helper = payload_bytes;
            self.last_bulk_ts = timestamp_us;
        } else {
            // Check if idle time > 1.0 second (1,000,000 microseconds)
            let gap = (timestamp_us - self.last_bulk_ts) as f64 / 1_000_000.0;
            if gap > 1.0 {
                self.start_helper = timestamp_us;
                self.last_bulk_ts = timestamp_us;
                self.packet_count_helper = 1;
                self.size_helper = payload_bytes;
            } else {
                self.packet_count_helper += 1;
                self.size_helper += payload_bytes;

                if self.packet_count_helper == 4 {
                    self.state_count += 1;
                    self.packet_count += self.packet_count_helper;
                    self.size_total += self.size_helper;
                    self.duration += timestamp_us - self.start_helper;
                } else if self.packet_count_helper > 4 {
                    self.packet_count += 1;
                    self.size_total += payload_bytes;
                    self.duration += timestamp_us - self.last_bulk_ts;
                }
                self.last_bulk_ts = timestamp_us;
            }
        }
    }

    #[inline]
    pub fn avg_bytes_per_bulk(&self) -> u64 {
        if self.state_count != 0 {
            self.size_total / self.state_count
        } else {
            0
        }
    }

    #[inline]
    pub fn avg_packets_per_bulk(&self) -> u64 {
        if self.state_count != 0 {
            self.packet_count / self.state_count
        } else {
            0
        }
    }

    #[inline]
    pub fn avg_bulk_rate(&self) -> u64 {
        if self.duration != 0 {
            let dur_sec = self.duration as f64 / 1_000_000.0;
            (self.size_total as f64 / dur_sec) as u64
        } else {
            0
        }
    }
}

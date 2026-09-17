use crate::flow::stats::SummaryStats;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SubflowTracker {
    pub sf_last_packet_ts: i64,
    pub sf_count: u64,
    pub sf_ac_helper: i64,
    pub start_active_time: i64,
    pub end_active_time: i64,
    pub flow_active_stats: SummaryStats,
    pub flow_idle_stats: SummaryStats,
    pub activity_timeout: i64, // microseconds (default 5,000,000)
}

impl SubflowTracker {
    pub fn new(activity_timeout: i64) -> Self {
        Self {
            sf_last_packet_ts: -1,
            sf_count: 0,
            sf_ac_helper: -1,
            start_active_time: 0,
            end_active_time: 0,
            flow_active_stats: SummaryStats::new(),
            flow_idle_stats: SummaryStats::new(),
            activity_timeout,
        }
    }

    pub fn first_packet(&mut self, timestamp_us: i64) {
        self.sf_last_packet_ts = timestamp_us;
        self.sf_ac_helper = timestamp_us;
        self.start_active_time = timestamp_us;
        self.end_active_time = timestamp_us;
    }

    pub fn update_subflow(&mut self, timestamp_us: i64) {
        if self.sf_last_packet_ts == -1 {
            self.first_packet(timestamp_us);
            return;
        }

        let gap = (timestamp_us - self.sf_last_packet_ts) as f64 / 1_000_000.0;
        if gap > 1.0 {
            self.sf_count += 1;
            self.update_active_idle_time(timestamp_us);
            self.sf_ac_helper = timestamp_us;
        }

        self.sf_last_packet_ts = timestamp_us;
    }

    pub fn update_active_idle_time(&mut self, current_time: i64) {
        if (current_time - self.end_active_time) > self.activity_timeout {
            if (self.end_active_time - self.start_active_time) > 0 {
                self.flow_active_stats
                    .add_value((self.end_active_time - self.start_active_time) as f64);
            }
            self.flow_idle_stats
                .add_value((current_time - self.end_active_time) as f64);
            self.start_active_time = current_time;
            self.end_active_time = current_time;
        } else {
            self.end_active_time = current_time;
        }
    }

    pub fn end_active_idle_time(
        &mut self,
        _current_time: i64,
        flow_start_time: i64,
        flow_timeout: i64,
        is_flag_end: bool,
    ) {
        if (self.end_active_time - self.start_active_time) > 0 {
            self.flow_active_stats
                .add_value((self.end_active_time - self.start_active_time) as f64);
        }

        if !is_flag_end {
            let active_span = self.end_active_time - flow_start_time;
            if flow_timeout > active_span {
                self.flow_idle_stats
                    .add_value((flow_timeout - active_span) as f64);
            }
        }
    }
}

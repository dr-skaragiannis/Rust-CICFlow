use crate::flow::bulk::BulkTracker;
use crate::flow::feature::FlowFeatures;
use crate::flow::stats::SummaryStats;
use crate::flow::subflow::SubflowTracker;
use crate::packet::info::BasicPacketInfo;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::net::IpAddr;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BasicFlow {
    pub is_bidirectional: bool,
    pub src_ip: IpAddr,
    pub dst_ip: IpAddr,
    pub src_bytes: Vec<u8>,
    pub dst_bytes: Vec<u8>,
    pub src_port: u16,
    pub dst_port: u16,
    pub protocol: u8,
    pub flow_id: String,
    pub flow_start_time: i64, // microseconds
    pub flow_last_seen: i64,  // microseconds
    pub forward_last_seen: i64,
    pub backward_last_seen: i64,

    pub fwd_pkt_stats: SummaryStats,
    pub bwd_pkt_stats: SummaryStats,
    pub flow_iat: SummaryStats,
    pub forward_iat: SummaryStats,
    pub backward_iat: SummaryStats,
    pub flow_length_stats: SummaryStats,

    pub fwd_packet_count: u64,
    pub bwd_packet_count: u64,
    pub forward_bytes: u64,
    pub backward_bytes: u64,
    pub f_header_bytes: u64,
    pub b_header_bytes: u64,

    pub f_psh_cnt: u32,
    pub b_psh_cnt: u32,
    pub f_urg_cnt: u32,
    pub b_urg_cnt: u32,
    pub f_fin_cnt: u32,
    pub b_fin_cnt: u32,

    pub flag_fin: u32,
    pub flag_syn: u32,
    pub flag_rst: u32,
    pub flag_psh: u32,
    pub flag_ack: u32,
    pub flag_urg: u32,
    pub flag_cwr: u32,
    pub flag_ece: u32,

    pub act_data_pkt_forward: u64,
    pub min_seg_size_forward: u64,
    pub init_win_bytes_forward: u32,
    pub init_win_bytes_backward: u32,

    pub fwd_bulk: BulkTracker,
    pub bwd_bulk: BulkTracker,
    pub subflow: SubflowTracker,

    pub label: String,
    pub compat_mode: bool,
}

impl BasicFlow {
    pub fn new(
        packet: &BasicPacketInfo,
        is_bidirectional: bool,
        activity_timeout: i64,
        label: Option<&str>,
        compat_mode: bool,
    ) -> Self {
        let mut flow = Self {
            is_bidirectional,
            src_ip: packet.src_ip,
            dst_ip: packet.dst_ip,
            src_bytes: packet.src_bytes.clone(),
            dst_bytes: packet.dst_bytes.clone(),
            src_port: packet.src_port,
            dst_port: packet.dst_port,
            protocol: packet.protocol,
            flow_id: packet.generate_flow_id(compat_mode),
            flow_start_time: packet.timestamp,
            flow_last_seen: packet.timestamp,
            forward_last_seen: packet.timestamp,
            backward_last_seen: 0,
            fwd_pkt_stats: SummaryStats::new(),
            bwd_pkt_stats: SummaryStats::new(),
            flow_iat: SummaryStats::new(),
            forward_iat: SummaryStats::new(),
            backward_iat: SummaryStats::new(),
            flow_length_stats: SummaryStats::new(),
            fwd_packet_count: 0,
            bwd_packet_count: 0,
            forward_bytes: 0,
            backward_bytes: 0,
            f_header_bytes: 0,
            b_header_bytes: 0,
            f_psh_cnt: 0,
            b_psh_cnt: 0,
            f_urg_cnt: 0,
            b_urg_cnt: 0,
            f_fin_cnt: 0,
            b_fin_cnt: 0,
            flag_fin: 0,
            flag_syn: 0,
            flag_rst: 0,
            flag_psh: 0,
            flag_ack: 0,
            flag_urg: 0,
            flag_cwr: 0,
            flag_ece: 0,
            act_data_pkt_forward: 0,
            min_seg_size_forward: u64::MAX,
            init_win_bytes_forward: 0,
            init_win_bytes_backward: 0,
            fwd_bulk: BulkTracker::new(),
            bwd_bulk: BulkTracker::new(),
            subflow: SubflowTracker::new(activity_timeout),
            label: label.unwrap_or("NeedManualLabel").to_string(),
            compat_mode,
        };

        flow.first_packet(packet);
        flow
    }

    pub fn first_packet(&mut self, packet: &BasicPacketInfo) {
        self.update_bulk(packet);
        self.subflow.update_subflow(packet.timestamp);
        self.check_flags(packet);

        self.flow_start_time = packet.timestamp;
        self.flow_last_seen = packet.timestamp;
        self.subflow.first_packet(packet.timestamp);

        if self.compat_mode {
            // Replicate original Java bug: line 129 adds packet length before checking direction
            self.flow_length_stats.add_value(packet.payload_bytes as f64);
        }

        if packet.is_forward_packet(&self.src_bytes) {
            self.min_seg_size_forward = packet.header_bytes;
            self.init_win_bytes_forward = packet.tcp_window;
            self.flow_length_stats.add_value(packet.payload_bytes as f64);
            self.fwd_pkt_stats.add_value(packet.payload_bytes as f64);
            self.f_header_bytes = packet.header_bytes;
            self.forward_last_seen = packet.timestamp;
            self.forward_bytes += packet.payload_bytes;
            self.fwd_packet_count += 1;
            if packet.flag_psh {
                self.f_psh_cnt += 1;
            }
            if packet.flag_urg {
                self.f_urg_cnt += 1;
            }
        } else {
            self.init_win_bytes_backward = packet.tcp_window;
            self.flow_length_stats.add_value(packet.payload_bytes as f64);
            self.bwd_pkt_stats.add_value(packet.payload_bytes as f64);
            self.b_header_bytes = packet.header_bytes;
            self.backward_last_seen = packet.timestamp;
            self.backward_bytes += packet.payload_bytes;
            self.bwd_packet_count += 1;
            if packet.flag_psh {
                self.b_psh_cnt += 1;
            }
            if packet.flag_urg {
                self.b_urg_cnt += 1;
            }
        }
    }

    pub fn add_packet(&mut self, packet: &BasicPacketInfo) {
        self.update_bulk(packet);
        self.subflow.update_subflow(packet.timestamp);
        self.check_flags(packet);

        let current_ts = packet.timestamp;

        if self.is_bidirectional {
            self.flow_length_stats.add_value(packet.payload_bytes as f64);

            if packet.is_forward_packet(&self.src_bytes) {
                if packet.payload_bytes >= 1 {
                    self.act_data_pkt_forward += 1;
                }
                self.fwd_pkt_stats.add_value(packet.payload_bytes as f64);
                self.f_header_bytes += packet.header_bytes;
                self.forward_bytes += packet.payload_bytes;
                self.fwd_packet_count += 1;

                if self.fwd_packet_count > 1 {
                    self.forward_iat.add_value((current_ts - self.forward_last_seen) as f64);
                }
                self.forward_last_seen = current_ts;
                self.min_seg_size_forward = self.min_seg_size_forward.min(packet.header_bytes);
            } else {
                self.bwd_pkt_stats.add_value(packet.payload_bytes as f64);
                self.init_win_bytes_backward = packet.tcp_window;
                self.b_header_bytes += packet.header_bytes;
                self.backward_bytes += packet.payload_bytes;
                self.bwd_packet_count += 1;

                if self.bwd_packet_count > 1 {
                    self.backward_iat.add_value((current_ts - self.backward_last_seen) as f64);
                }
                self.backward_last_seen = current_ts;
            }
        } else {
            if packet.payload_bytes >= 1 {
                self.act_data_pkt_forward += 1;
            }
            self.fwd_pkt_stats.add_value(packet.payload_bytes as f64);
            self.flow_length_stats.add_value(packet.payload_bytes as f64);
            self.f_header_bytes += packet.header_bytes;
            self.forward_bytes += packet.payload_bytes;
            self.fwd_packet_count += 1;

            if self.fwd_packet_count > 1 {
                self.forward_iat.add_value((current_ts - self.forward_last_seen) as f64);
            }
            self.forward_last_seen = current_ts;
            self.min_seg_size_forward = self.min_seg_size_forward.min(packet.header_bytes);
        }

        self.flow_iat.add_value((packet.timestamp - self.flow_last_seen) as f64);
        self.flow_last_seen = packet.timestamp;
    }

    fn check_flags(&mut self, packet: &BasicPacketInfo) {
        if packet.flag_fin {
            self.flag_fin += 1;
        }
        if packet.flag_syn {
            self.flag_syn += 1;
        }
        if packet.flag_rst {
            self.flag_rst += 1;
        }
        if packet.flag_psh {
            self.flag_psh += 1;
        }
        if packet.flag_ack {
            self.flag_ack += 1;
        }
        if packet.flag_urg {
            self.flag_urg += 1;
        }
        if packet.flag_cwr {
            self.flag_cwr += 1;
        }
        if packet.flag_ece {
            self.flag_ece += 1;
        }
    }

    fn update_bulk(&mut self, packet: &BasicPacketInfo) {
        if packet.is_forward_packet(&self.src_bytes) {
            let b_last = self.bwd_bulk.last_bulk_ts;
            self.fwd_bulk.update(packet.timestamp, packet.payload_bytes, b_last);
        } else {
            let f_last = self.fwd_bulk.last_bulk_ts;
            self.bwd_bulk.update(packet.timestamp, packet.payload_bytes, f_last);
        }
    }

    #[inline]
    pub fn packet_count(&self) -> u64 {
        if self.is_bidirectional {
            self.fwd_packet_count + self.bwd_packet_count
        } else {
            self.fwd_packet_count
        }
    }

    #[inline]
    pub fn flow_duration(&self) -> i64 {
        self.flow_last_seen - self.flow_start_time
    }

    #[inline]
    pub fn fwd_pkts_per_sec(&self) -> f64 {
        let dur = self.flow_duration();
        if dur > 0 {
            (self.fwd_packet_count as f64) / ((dur as f64) / 1_000_000.0)
        } else {
            0.0
        }
    }

    #[inline]
    pub fn bwd_pkts_per_sec(&self) -> f64 {
        let dur = self.flow_duration();
        if dur > 0 {
            (self.bwd_packet_count as f64) / ((dur as f64) / 1_000_000.0)
        } else {
            0.0
        }
    }

    #[inline]
    pub fn down_up_ratio(&self) -> f64 {
        if self.fwd_packet_count > 0 {
            (self.bwd_packet_count as f64) / (self.fwd_packet_count as f64)
        } else {
            0.0
        }
    }

    #[inline]
    pub fn avg_packet_size(&self) -> f64 {
        let count = self.packet_count();
        if count > 0 {
            self.flow_length_stats.sum() / (count as f64)
        } else {
            0.0
        }
    }

    #[inline]
    pub fn fwd_avg_segment_size(&self) -> f64 {
        if self.fwd_packet_count > 0 {
            self.fwd_pkt_stats.sum() / (self.fwd_packet_count as f64)
        } else {
            0.0
        }
    }

    #[inline]
    pub fn bwd_avg_segment_size(&self) -> f64 {
        if self.bwd_packet_count > 0 {
            self.bwd_pkt_stats.sum() / (self.bwd_packet_count as f64)
        } else {
            0.0
        }
    }

    #[inline]
    pub fn subflow_fwd_packets(&self) -> u64 {
        if self.subflow.sf_count == 0 {
            0
        } else {
            self.fwd_packet_count / self.subflow.sf_count
        }
    }

    #[inline]
    pub fn subflow_fwd_bytes(&self) -> u64 {
        if self.subflow.sf_count == 0 {
            0
        } else {
            self.forward_bytes / self.subflow.sf_count
        }
    }

    #[inline]
    pub fn subflow_bwd_packets(&self) -> u64 {
        if self.subflow.sf_count == 0 {
            0
        } else {
            self.bwd_packet_count / self.subflow.sf_count
        }
    }

    #[inline]
    pub fn subflow_bwd_bytes(&self) -> u64 {
        if self.subflow.sf_count == 0 {
            0
        } else {
            self.backward_bytes / self.subflow.sf_count
        }
    }

    #[inline]
    pub fn set_fwd_fin(&mut self) -> u32 {
        self.f_fin_cnt += 1;
        self.f_fin_cnt
    }

    #[inline]
    pub fn set_bwd_fin(&mut self) -> u32 {
        self.b_fin_cnt += 1;
        self.b_fin_cnt
    }

    pub fn formatted_timestamp(&self) -> String {
        let millis = self.flow_start_time / 1000;
        if let Some(dt) = DateTime::from_timestamp_millis(millis) {
            let utc_dt: DateTime<Utc> = dt;
            utc_dt.format("%d/%m/%Y %I:%M:%S %p").to_string()
        } else {
            "01/01/1970 12:00:00 AM".to_string()
        }
    }

    pub fn extract_features(&self) -> FlowFeatures {
        let flow_duration = self.flow_duration();
        let dur_sec = (flow_duration as f64) / 1_000_000.0;

        let flow_bytes_sec = if dur_sec > 0.0 {
            ((self.forward_bytes + self.backward_bytes) as f64) / dur_sec
        } else {
            0.0
        };

        let flow_pkts_sec = if dur_sec > 0.0 {
            (self.packet_count() as f64) / dur_sec
        } else {
            0.0
        };

        let fwd_iat_tot = if self.fwd_packet_count > 1 {
            self.forward_iat.sum()
        } else {
            0.0
        };
        let fwd_iat_mean = if self.fwd_packet_count > 1 {
            self.forward_iat.mean()
        } else {
            0.0
        };
        let fwd_iat_std = if self.fwd_packet_count > 1 {
            self.forward_iat.std_dev()
        } else {
            0.0
        };
        let fwd_iat_max = if self.fwd_packet_count > 1 {
            self.forward_iat.max()
        } else {
            0.0
        };
        let fwd_iat_min = if self.fwd_packet_count > 1 {
            self.forward_iat.min()
        } else {
            0.0
        };

        let bwd_iat_tot = if self.bwd_packet_count > 1 {
            self.backward_iat.sum()
        } else {
            0.0
        };
        let bwd_iat_mean = if self.bwd_packet_count > 1 {
            self.backward_iat.mean()
        } else {
            0.0
        };
        let bwd_iat_std = if self.bwd_packet_count > 1 {
            self.backward_iat.std_dev()
        } else {
            0.0
        };
        let bwd_iat_max = if self.bwd_packet_count > 1 {
            self.backward_iat.max()
        } else {
            0.0
        };
        let bwd_iat_min = if self.bwd_packet_count > 1 {
            self.backward_iat.min()
        } else {
            0.0
        };

        let (pkt_len_min, pkt_len_max, pkt_len_mean, pkt_len_std, pkt_len_var) =
            if self.packet_count() > 0 {
                (
                    self.flow_length_stats.min(),
                    self.flow_length_stats.max(),
                    self.flow_length_stats.mean(),
                    self.flow_length_stats.std_dev(),
                    self.flow_length_stats.variance(),
                )
            } else {
                (0.0, 0.0, 0.0, 0.0, 0.0)
            };

        let min_seg = if self.min_seg_size_forward == u64::MAX {
            0
        } else {
            self.min_seg_size_forward
        };

        FlowFeatures {
            flow_id: self.flow_id.clone(),
            src_ip: self.src_ip.to_string(),
            src_port: self.src_port,
            dst_ip: self.dst_ip.to_string(),
            dst_port: self.dst_port,
            protocol: self.protocol,
            timestamp: self.formatted_timestamp(),
            flow_duration,
            tot_fwd_pkts: self.fwd_packet_count,
            tot_bwd_pkts: self.bwd_packet_count,
            tot_len_fwd_pkts: self.fwd_pkt_stats.sum(),
            tot_len_bwd_pkts: self.bwd_pkt_stats.sum(),
            fwd_pkt_len_max: self.fwd_pkt_stats.max(),
            fwd_pkt_len_min: self.fwd_pkt_stats.min(),
            fwd_pkt_len_mean: self.fwd_pkt_stats.mean(),
            fwd_pkt_len_std: self.fwd_pkt_stats.std_dev(),
            bwd_pkt_len_max: self.bwd_pkt_stats.max(),
            bwd_pkt_len_min: self.bwd_pkt_stats.min(),
            bwd_pkt_len_mean: self.bwd_pkt_stats.mean(),
            bwd_pkt_len_std: self.bwd_pkt_stats.std_dev(),
            flow_bytes_sec,
            flow_pkts_sec,
            flow_iat_mean: self.flow_iat.mean(),
            flow_iat_std: self.flow_iat.std_dev(),
            flow_iat_max: self.flow_iat.max(),
            flow_iat_min: self.flow_iat.min(),
            fwd_iat_tot,
            fwd_iat_mean,
            fwd_iat_std,
            fwd_iat_max,
            fwd_iat_min,
            bwd_iat_tot,
            bwd_iat_mean,
            bwd_iat_std,
            bwd_iat_max,
            bwd_iat_min,
            fwd_psh_flags: self.f_psh_cnt,
            bwd_psh_flags: self.b_psh_cnt,
            fwd_urg_flags: self.f_urg_cnt,
            bwd_urg_flags: self.b_urg_cnt,
            fwd_hdr_len: self.f_header_bytes,
            bwd_hdr_len: self.b_header_bytes,
            fwd_pkts_sec: self.fwd_pkts_per_sec(),
            bwd_pkts_sec: self.bwd_pkts_per_sec(),
            pkt_len_min,
            pkt_len_max,
            pkt_len_mean,
            pkt_len_std,
            pkt_len_var,
            fin_cnt: self.flag_fin,
            syn_cnt: self.flag_syn,
            rst_cnt: self.flag_rst,
            psh_cnt: self.flag_psh,
            ack_cnt: self.flag_ack,
            urg_cnt: self.flag_urg,
            cwr_cnt: self.flag_cwr,
            ece_cnt: self.flag_ece,
            down_up_ratio: self.down_up_ratio(),
            pkt_size_avg: self.avg_packet_size(),
            fwd_seg_avg: self.fwd_avg_segment_size(),
            bwd_seg_avg: self.bwd_avg_segment_size(),
            fwd_bytes_bulk_avg: self.fwd_bulk.avg_bytes_per_bulk(),
            fwd_pkt_bulk_avg: self.fwd_bulk.avg_packets_per_bulk(),
            fwd_bulk_rate_avg: self.fwd_bulk.avg_bulk_rate(),
            bwd_bytes_bulk_avg: self.bwd_bulk.avg_bytes_per_bulk(),
            bwd_pkt_bulk_avg: self.bwd_bulk.avg_packets_per_bulk(),
            bwd_bulk_rate_avg: self.bwd_bulk.avg_bulk_rate(),
            subfl_fwd_pkt: self.subflow_fwd_packets(),
            subfl_fwd_byt: self.subflow_fwd_bytes(),
            subfl_bwd_pkt: self.subflow_bwd_packets(),
            subfl_bwd_byt: self.subflow_bwd_bytes(),
            fwd_win_byt: self.init_win_bytes_forward,
            bwd_win_byt: self.init_win_bytes_backward,
            fwd_act_pkt: self.act_data_pkt_forward,
            fwd_seg_min: min_seg,
            active_mean: self.subflow.flow_active_stats.mean(),
            active_std: self.subflow.flow_active_stats.std_dev(),
            active_max: self.subflow.flow_active_stats.max(),
            active_min: self.subflow.flow_active_stats.min(),
            idle_mean: self.subflow.flow_idle_stats.mean(),
            idle_std: self.subflow.flow_idle_stats.std_dev(),
            idle_max: self.subflow.flow_idle_stats.max(),
            idle_min: self.subflow.flow_idle_stats.min(),
            label: self.label.clone(),
        }
    }
}

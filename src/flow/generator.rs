use crate::flow::basic_flow::BasicFlow;
use crate::packet::info::BasicPacketInfo;
use ahash::AHashMap;
use std::sync::Arc;

pub type FlowCallback = Arc<dyn Fn(BasicFlow) + Send + Sync>;

pub struct FlowGenerator {
    bidirectional: bool,
    flow_timeout: i64,      // microseconds (default 120,000,000)
    activity_timeout: i64,  // microseconds (default 5,000,000)
    current_flows: AHashMap<String, BasicFlow>,
    finished_flows: Vec<BasicFlow>,
    listener: Option<FlowCallback>,
    finished_flow_count: u64,
    label: Option<String>,
    compat_mode: bool,
    min_packets_per_flow: u64,
}

impl FlowGenerator {
    pub fn new(
        bidirectional: bool,
        flow_timeout: i64,
        activity_timeout: i64,
        label: Option<String>,
        compat_mode: bool,
    ) -> Self {
        Self {
            bidirectional,
            flow_timeout,
            activity_timeout,
            current_flows: AHashMap::new(),
            finished_flows: Vec::new(),
            listener: None,
            finished_flow_count: 0,
            label,
            compat_mode,
            min_packets_per_flow: 2, // CICFlowMeter standard ignores 1-packet flows
        }
    }

    pub fn set_min_packets_per_flow(&mut self, min_pkts: u64) {
        self.min_packets_per_flow = min_pkts;
    }

    pub fn set_flow_listener<F>(&mut self, callback: F)
    where
        F: Fn(BasicFlow) + Send + Sync + 'static,
    {
        self.listener = Some(Arc::new(callback));
    }

    pub fn add_packet(&mut self, packet: BasicPacketInfo) {
        let fwd_id = packet.fwd_flow_id();
        let bwd_id = packet.bwd_flow_id();
        let current_ts = packet.timestamp;

        let existing_key = if self.current_flows.contains_key(&fwd_id) {
            Some(fwd_id.clone())
        } else if self.current_flows.contains_key(&bwd_id) {
            Some(bwd_id.clone())
        } else {
            None
        };

        if let Some(key) = existing_key {
            let mut flow = self.current_flows.remove(&key).unwrap();

            // Check flow timeout
            if (current_ts - flow.flow_start_time) > self.flow_timeout {
                if flow.packet_count() >= self.min_packets_per_flow {
                    self.emit_flow(flow);
                }

                // Create new flow with current packet
                let new_flow = BasicFlow::new(
                    &packet,
                    self.bidirectional,
                    self.activity_timeout,
                    self.label.as_deref(),
                    self.compat_mode,
                );
                self.current_flows.insert(fwd_id, new_flow);
            } else if packet.flag_fin {
                // Check FIN exchange
                let is_forward = packet.is_forward_packet(&flow.src_bytes);
                if is_forward {
                    if flow.set_fwd_fin() == 1 {
                        if (flow.f_fin_cnt + flow.b_fin_cnt) == 2 {
                            flow.add_packet(&packet);
                            if flow.packet_count() >= self.min_packets_per_flow {
                                self.emit_flow(flow);
                            }
                        } else {
                            flow.subflow.update_active_idle_time(current_ts);
                            flow.add_packet(&packet);
                            self.current_flows.insert(key, flow);
                        }
                    } else {
                        // Already had fwd FIN
                        flow.add_packet(&packet);
                        self.current_flows.insert(key, flow);
                    }
                } else if flow.set_bwd_fin() == 1 {
                    if (flow.f_fin_cnt + flow.b_fin_cnt) == 2 {
                        flow.add_packet(&packet);
                        if flow.packet_count() >= self.min_packets_per_flow {
                            self.emit_flow(flow);
                        }
                    } else {
                        flow.subflow.update_active_idle_time(current_ts);
                        flow.add_packet(&packet);
                        self.current_flows.insert(key, flow);
                    }
                } else {
                    // Already had bwd FIN
                    flow.add_packet(&packet);
                    self.current_flows.insert(key, flow);
                }
            } else if packet.flag_rst {
                flow.add_packet(&packet);
                if flow.packet_count() >= self.min_packets_per_flow {
                    self.emit_flow(flow);
                }
            } else {
                let is_fwd = packet.is_forward_packet(&flow.src_bytes);
                if is_fwd && flow.f_fin_cnt == 0 {
                    flow.subflow.update_active_idle_time(current_ts);
                    flow.add_packet(&packet);
                    self.current_flows.insert(key, flow);
                } else if !is_fwd && flow.b_fin_cnt == 0 {
                    flow.subflow.update_active_idle_time(current_ts);
                    flow.add_packet(&packet);
                    self.current_flows.insert(key, flow);
                } else {
                    // Flow is partially closed
                    flow.add_packet(&packet);
                    self.current_flows.insert(key, flow);
                }
            }
        } else {
            let flow = BasicFlow::new(
                &packet,
                self.bidirectional,
                self.activity_timeout,
                self.label.as_deref(),
                self.compat_mode,
            );
            self.current_flows.insert(fwd_id, flow);
        }
    }

    fn emit_flow(&mut self, flow: BasicFlow) {
        self.finished_flow_count += 1;
        if let Some(ref listener) = self.listener {
            listener(flow);
        } else {
            self.finished_flows.push(flow);
        }
    }

    /// Flushes all remaining active flows at the end of capture
    pub fn finish_all_flows(&mut self) -> Vec<BasicFlow> {
        let mut flushed = Vec::new();
        for (_, flow) in self.current_flows.drain() {
            if flow.packet_count() >= self.min_packets_per_flow {
                if let Some(ref listener) = self.listener {
                    listener(flow);
                } else {
                    flushed.push(flow);
                }
                self.finished_flow_count += 1;
            }
        }
        flushed
    }

    pub fn get_finished_flows(&mut self) -> Vec<BasicFlow> {
        std::mem::take(&mut self.finished_flows)
    }

    pub fn total_finished_flows(&self) -> u64 {
        self.finished_flow_count
    }

    pub fn active_flow_count(&self) -> usize {
        self.current_flows.len()
    }
}

use cicflowmeter::flow::feature::FlowFeatures;
use cicflowmeter::flow::generator::FlowGenerator;
use cicflowmeter::packet::info::BasicPacketInfo;
use std::net::IpAddr;

/// Helper function to convert extracted flow features into a numeric vector (tensor) for ML models
pub fn features_to_feature_vector(f: &FlowFeatures) -> Vec<f64> {
    vec![
        f.flow_duration as f64,
        f.tot_fwd_pkts as f64,
        f.tot_bwd_pkts as f64,
        f.tot_len_fwd_pkts,
        f.tot_len_bwd_pkts,
        f.fwd_pkt_len_max,
        f.fwd_pkt_len_min,
        f.fwd_pkt_len_mean,
        f.fwd_pkt_len_std,
        f.bwd_pkt_len_max,
        f.bwd_pkt_len_min,
        f.bwd_pkt_len_mean,
        f.bwd_pkt_len_std,
        f.flow_bytes_sec,
        f.flow_pkts_sec,
        f.flow_iat_mean,
        f.flow_iat_std,
        f.flow_iat_max,
        f.flow_iat_min,
        f.fwd_iat_tot,
        f.fwd_iat_mean,
        f.fwd_iat_std,
        f.fwd_iat_max,
        f.fwd_iat_min,
        f.bwd_iat_tot,
        f.bwd_iat_mean,
        f.bwd_iat_std,
        f.bwd_iat_max,
        f.bwd_iat_min,
        f.fwd_psh_flags as f64,
        f.bwd_psh_flags as f64,
        f.fwd_urg_flags as f64,
        f.bwd_urg_flags as f64,
        f.fwd_hdr_len as f64,
        f.bwd_hdr_len as f64,
        f.fwd_pkts_sec,
        f.bwd_pkts_sec,
        f.pkt_len_min,
        f.pkt_len_max,
        f.pkt_len_mean,
        f.pkt_len_std,
        f.pkt_len_var,
        f.fin_cnt as f64,
        f.syn_cnt as f64,
        f.rst_cnt as f64,
        f.psh_cnt as f64,
        f.ack_cnt as f64,
        f.urg_cnt as f64,
        f.cwr_cnt as f64,
        f.ece_cnt as f64,
        f.down_up_ratio,
        f.pkt_size_avg,
        f.fwd_seg_avg,
        f.bwd_seg_avg,
        f.fwd_bytes_bulk_avg as f64,
        f.fwd_pkt_bulk_avg as f64,
        f.fwd_bulk_rate_avg as f64,
        f.bwd_bytes_bulk_avg as f64,
        f.bwd_pkt_bulk_avg as f64,
        f.bwd_bulk_rate_avg as f64,
        f.subfl_fwd_pkt as f64,
        f.subfl_fwd_byt as f64,
        f.subfl_bwd_pkt as f64,
        f.subfl_bwd_byt as f64,
        f.fwd_win_byt as f64,
        f.bwd_win_byt as f64,
        f.fwd_act_pkt as f64,
        f.fwd_seg_min as f64,
        f.active_mean,
        f.active_std,
        f.active_max,
        f.active_min,
        f.idle_mean,
        f.idle_std,
        f.idle_max,
        f.idle_min,
    ]
}

fn main() {
    println!("==================================================================");
    println!(" CICFlowMeter Rust: Machine Learning Feature Vector Pipeline");
    println!("==================================================================");

    let src_ip: IpAddr = "192.168.1.100".parse().unwrap();
    let dst_ip: IpAddr = "10.0.0.1".parse().unwrap();

    let mut generator = FlowGenerator::new(true, 120_000_000, 5_000_000, Some("ANOMALY".to_string()), false);

    // Simulate SYN Flood anomaly traffic
    for i in 0..10 {
        let p = BasicPacketInfo::new(
            i + 1,
            src_ip,
            dst_ip,
            vec![192, 168, 1, 100],
            vec![10, 0, 0, 1],
            40000,
            80,
            6,
            (i as i64) * 1000,
            0,
            20,
            64240,
            (false, true, false, false, false, false, false, false), // SYN
        );
        generator.add_packet(p);
    }

    let flows = generator.finish_all_flows();
    for flow in flows {
        let features = flow.extract_features();
        let vector = features_to_feature_vector(&features);

        println!("Flow ID           : {}", features.flow_id);
        println!("Label             : {}", features.label);
        println!("Feature Vector Dim: {}", vector.len());
        println!("Sample Values (First 10 Dimensions): {:?}", &vector[0..10]);
        println!("SYN Flag Count    : {}", features.syn_cnt);
        println!("Flow Packets/sec  : {:.2}", features.flow_pkts_sec);
    }
}

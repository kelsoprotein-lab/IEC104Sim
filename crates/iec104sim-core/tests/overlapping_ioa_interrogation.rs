//! Regression test for the "GI/CI overwrite each other at shared IOA" bug.
//!
//! Backend keys `DataPointMap` by `(ioa, asdu_type)`. Two points at the same
//! IOA but different ASDU types (e.g. M_ME_NC_1 short-float + M_IT_NA_1 counter)
//! must coexist across GI and CI responses — neither should evict the other.
//!
//! The master-frontend bug with the same symptom (浮点 / 累计量 history消失) was
//! caused by the UI keying its local map by IOA only; the fix in
//! `master-frontend/src/components/DataTable.vue` now uses a composite
//! `${ioa}|${asdu_type}` key.

use iec104sim_core::data_point::{DataPoint, DataPointValue, InformationObjectDef};
use iec104sim_core::master::{MasterConfig, MasterConnection};
use iec104sim_core::slave::{SlaveServer, SlaveTransportConfig, Station};
use iec104sim_core::types::{AsduTypeId, DataCategory};
use tokio::time::{sleep, Duration};

fn free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

fn build_station_with_overlap() -> Station {
    let mut st = Station::new(1, "overlap");
    // Float (ME_NC) and Counter (IT) sharing IOA 100 + 101.
    for ioa in [100u32, 101] {
        st.object_defs.push(InformationObjectDef {
            ioa,
            asdu_type: AsduTypeId::MMeNc1,
            category: DataCategory::FloatMeasured,
            name: String::new(),
            comment: String::new(),
        });
        st.data_points.insert(DataPoint::with_value(
            ioa,
            AsduTypeId::MMeNc1,
            DataPointValue::ShortFloat { value: 12.5 + ioa as f32 },
        ));

        st.object_defs.push(InformationObjectDef {
            ioa,
            asdu_type: AsduTypeId::MItNa1,
            category: DataCategory::IntegratedTotals,
            name: String::new(),
            comment: String::new(),
        });
        st.data_points.insert(DataPoint::with_value(
            ioa,
            AsduTypeId::MItNa1,
            DataPointValue::IntegratedTotal { value: 1000 + ioa as i32, carry: false, sequence: 0 },
        ));
    }
    st
}

#[tokio::test]
async fn gi_then_ci_keeps_float_and_counter_at_same_ioa() {
    let port = free_port();
    let mut slave = SlaveServer::new(SlaveTransportConfig {
        bind_address: "127.0.0.1".into(),
        port,
        ..Default::default()
    });
    slave.add_station(build_station_with_overlap()).await.unwrap();
    slave.start().await.unwrap();
    sleep(Duration::from_millis(300)).await;

    let mut master = MasterConnection::new(MasterConfig {
        target_address: "127.0.0.1".into(),
        port,
        common_address: 1,
        ..Default::default()
    });
    master.connect().await.unwrap();
    sleep(Duration::from_millis(300)).await;

    // 1) General Interrogation — brings in both types.
    master.send_interrogation(1).await.unwrap();
    sleep(Duration::from_millis(1500)).await;
    {
        let data = master.received_data.read().await;
        let map = data.ca_map(1).expect("CA=1 map missing after GI");
        assert!(map.get(100, AsduTypeId::MMeNc1).is_some(), "float@100 missing after GI");
        assert!(map.get(100, AsduTypeId::MItNa1).is_some(), "counter@100 missing after GI");
        assert!(map.get(101, AsduTypeId::MMeNc1).is_some(), "float@101 missing after GI");
        assert!(map.get(101, AsduTypeId::MItNa1).is_some(), "counter@101 missing after GI");
    }

    // 2) Counter Interrogation — must NOT evict float entries at the same IOAs.
    master.send_counter_read(1).await.unwrap();
    sleep(Duration::from_millis(1500)).await;
    {
        let data = master.received_data.read().await;
        let map = data.ca_map(1).expect("CA=1 map missing after CI");
        assert!(map.get(100, AsduTypeId::MItNa1).is_some(), "counter@100 missing after CI");
        assert!(map.get(100, AsduTypeId::MMeNc1).is_some(),
            "float@100 should still exist after CI — backend must key by (ioa, asdu_type)");
        assert!(map.get(101, AsduTypeId::MItNa1).is_some(), "counter@101 missing after CI");
        assert!(map.get(101, AsduTypeId::MMeNc1).is_some(), "float@101 should still exist after CI");
    }

    // 3) Another GI — must NOT evict counter entries either.
    master.send_interrogation(1).await.unwrap();
    sleep(Duration::from_millis(1500)).await;
    {
        let data = master.received_data.read().await;
        let map = data.ca_map(1).expect("CA=1 map missing after GI#2");
        assert!(map.get(100, AsduTypeId::MMeNc1).is_some(), "float@100 missing after GI#2");
        assert!(map.get(100, AsduTypeId::MItNa1).is_some(),
            "counter@100 should still exist after GI#2");
    }

    master.disconnect().await.unwrap();
    slave.stop().await.unwrap();
}

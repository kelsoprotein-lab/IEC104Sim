use iec104sim_core::data_point::DataPointValue;
use iec104sim_core::master::{MasterConfig, MasterConnection};
use iec104sim_core::slave::{SlaveServer, SlaveTransportConfig, Station};
use iec104sim_core::types::AsduTypeId;
use tokio::time::{sleep, Duration};

fn free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

// =========================================================================
// Test: Single command direct execute — writeback to slave + spontaneous to master
// =========================================================================
#[tokio::test]
async fn test_single_command_writeback() {
    let port = free_port();

    // Start slave
    let transport = SlaveTransportConfig {
        bind_address: "127.0.0.1".to_string(),
        port,
        ..Default::default()
    };
    let mut slave = SlaveServer::new(transport);
    slave.add_station(Station::with_default_points(1, "Test", 2)).await.unwrap();
    slave.start().await.unwrap();
    sleep(Duration::from_millis(300)).await;

    // Connect master
    let config = MasterConfig {
        target_address: "127.0.0.1".to_string(),
        port,
        common_address: 1,
        ..Default::default()
    };
    let mut master = MasterConnection::new(config);
    master.connect().await.unwrap();
    sleep(Duration::from_millis(500)).await;

    // Send GI to populate master data
    master.send_interrogation(1).await.unwrap();
    sleep(Duration::from_millis(2000)).await;

    // Verify GI populated IOA=1
    {
        let data = master.received_data.read().await;
        let map = data.ca_map(1).expect("CA=1 map should exist after GI");
        assert!(map.get(1, AsduTypeId::MSpNa1).is_some(), "IOA=1 should exist after GI");
    }

    // Send single command: IOA=1, value=true, select=false (direct execute), QU=0, COT=6 (activation)
    master.send_single_command(1, true, false, 1, 0, 6).await.unwrap();
    sleep(Duration::from_millis(2000)).await;

    // Check slave memory was updated
    {
        let stations = slave.stations.read().await;
        let st = stations.get(&1).unwrap();
        let point = st.data_points.get(1, AsduTypeId::MSpNa1).unwrap();
        assert_eq!(point.value, DataPointValue::SinglePoint { value: true },
            "Slave data point should be SinglePoint(true)");
    }

    // Check master received the spontaneous update (COT=3)
    {
        let data = master.received_data.read().await;
        let map = data.ca_map(1).expect("CA=1 map should exist after writeback");
        let point = map.get(1, AsduTypeId::MSpNa1).unwrap();
        assert_eq!(point.value, DataPointValue::SinglePoint { value: true },
            "Master should see SinglePoint(true) via COT=3 writeback");
    }

    master.disconnect().await.unwrap();
    slave.stop().await.unwrap();
}

// =========================================================================
// Test: Double command writeback
// =========================================================================
#[tokio::test]
async fn test_double_command_writeback() {
    let port = free_port();

    let transport = SlaveTransportConfig {
        bind_address: "127.0.0.1".to_string(),
        port,
        ..Default::default()
    };
    let mut slave = SlaveServer::new(transport);
    slave.add_station(Station::with_default_points(1, "Test", 2)).await.unwrap();
    slave.start().await.unwrap();
    sleep(Duration::from_millis(300)).await;

    let config = MasterConfig {
        target_address: "127.0.0.1".to_string(),
        port,
        common_address: 1,
        ..Default::default()
    };
    let mut master = MasterConnection::new(config);
    master.connect().await.unwrap();
    sleep(Duration::from_millis(500)).await;

    master.send_interrogation(1).await.unwrap();
    sleep(Duration::from_millis(2000)).await;

    // IOA=3 is the first DoublePoint (2 SP + first DP), QU=0, COT=6
    master.send_double_command(3, 2, false, 1, 0, 6).await.unwrap();
    sleep(Duration::from_millis(2000)).await;

    {
        let stations = slave.stations.read().await;
        let point = stations.get(&1).unwrap().data_points.get(3, AsduTypeId::MDpNa1).unwrap();
        assert_eq!(point.value, DataPointValue::DoublePoint { value: 2 },
            "Slave DP IOA=3 should be 2");
    }

    {
        let data = master.received_data.read().await;
        let map = data.ca_map(1).expect("CA=1 map should exist");
        let point = map.get(3, AsduTypeId::MDpNa1).unwrap();
        assert_eq!(point.value, DataPointValue::DoublePoint { value: 2 },
            "Master should see DP=2 via writeback");
    }

    master.disconnect().await.unwrap();
    slave.stop().await.unwrap();
}

// =========================================================================
// Test: Setpoint float writeback
// =========================================================================
#[tokio::test]
async fn test_setpoint_float_writeback() {
    let port = free_port();

    let transport = SlaveTransportConfig {
        bind_address: "127.0.0.1".to_string(),
        port,
        ..Default::default()
    };
    let mut slave = SlaveServer::new(transport);
    slave.add_station(Station::with_default_points(1, "Test", 2)).await.unwrap();
    slave.start().await.unwrap();
    sleep(Duration::from_millis(300)).await;

    let config = MasterConfig {
        target_address: "127.0.0.1".to_string(),
        port,
        common_address: 1,
        ..Default::default()
    };
    let mut master = MasterConnection::new(config);
    master.connect().await.unwrap();
    sleep(Duration::from_millis(500)).await;

    master.send_interrogation(1).await.unwrap();
    sleep(Duration::from_millis(2000)).await;

    // IOA=13 is the first ShortFloat, QL=0, COT=6
    master.send_setpoint_float(13, 42.5, false, 1, 0, 6).await.unwrap();
    sleep(Duration::from_millis(2000)).await;

    {
        let stations = slave.stations.read().await;
        let point = stations.get(&1).unwrap().data_points.get(13, AsduTypeId::MMeNc1).unwrap();
        assert_eq!(point.value, DataPointValue::ShortFloat { value: 42.5 },
            "Slave float should be 42.5");
    }

    {
        let data = master.received_data.read().await;
        let map = data.ca_map(1).expect("CA=1 map should exist");
        let point = map.get(13, AsduTypeId::MMeNc1).unwrap();
        assert_eq!(point.value, DataPointValue::ShortFloat { value: 42.5 },
            "Master should see float=42.5 via writeback");
    }

    master.disconnect().await.unwrap();
    slave.stop().await.unwrap();
}

// =========================================================================
// Test: SbO (Select-before-Operate) single command
// =========================================================================
#[tokio::test]
async fn test_sbo_single_command() {
    let port = free_port();

    let transport = SlaveTransportConfig {
        bind_address: "127.0.0.1".to_string(),
        port,
        ..Default::default()
    };
    let mut slave = SlaveServer::new(transport);
    slave.add_station(Station::with_default_points(1, "Test", 2)).await.unwrap();
    slave.start().await.unwrap();
    sleep(Duration::from_millis(300)).await;

    let config = MasterConfig {
        target_address: "127.0.0.1".to_string(),
        port,
        common_address: 1,
        ..Default::default()
    };
    let mut master = MasterConnection::new(config);
    master.connect().await.unwrap();
    sleep(Duration::from_millis(500)).await;

    master.send_interrogation(1).await.unwrap();
    sleep(Duration::from_millis(2000)).await;

    // Build SbO frames for IOA=1, value=true
    let ca: u16 = 1;
    let ioa: u32 = 1;
    let ca_bytes = ca.to_le_bytes();
    let ioa_bytes = ioa.to_le_bytes();
    let select_frame = vec![
        0x68, 0x0E, 0x00, 0x00, 0x00, 0x00,
        45, 0x01, 6, 0x00,
        ca_bytes[0], ca_bytes[1],
        ioa_bytes[0], ioa_bytes[1], ioa_bytes[2],
        0x81, // value=true, select=true
    ];
    let execute_frame = vec![
        0x68, 0x0E, 0x00, 0x00, 0x00, 0x00,
        45, 0x01, 6, 0x00,
        ca_bytes[0], ca_bytes[1],
        ioa_bytes[0], ioa_bytes[1], ioa_bytes[2],
        0x01, // value=true, select=false
    ];

    let result = master.send_control_with_sbo(
        select_frame, execute_frame, ioa,
        "SbO单点",
        iec104sim_core::log_entry::FrameLabel::SingleCommand,
        ca,
    ).await;

    assert!(result.is_ok(), "SbO should succeed: {:?}", result.err());
    let r = result.unwrap();
    assert!(r.steps.len() >= 3, "SbO should have >=3 steps, got {}", r.steps.len());

    sleep(Duration::from_millis(2000)).await;

    {
        let stations = slave.stations.read().await;
        let point = stations.get(&1).unwrap().data_points.get(1, AsduTypeId::MSpNa1).unwrap();
        assert_eq!(point.value, DataPointValue::SinglePoint { value: true },
            "After SbO, slave should have SP=true");
    }

    master.disconnect().await.unwrap();
    slave.stop().await.unwrap();
}

// =========================================================================
// Test: Batch add data points with GI verification
// =========================================================================
#[tokio::test]
async fn test_batch_add_then_gi() {
    let port = free_port();

    let transport = SlaveTransportConfig {
        bind_address: "127.0.0.1".to_string(),
        port,
        ..Default::default()
    };
    let mut slave = SlaveServer::new(transport);
    let mut station = Station::new(1, "Test");
    station.batch_add_points(1, 10, AsduTypeId::MMeNc1, "FL").unwrap();
    slave.add_station(station).await.unwrap();
    slave.start().await.unwrap();
    sleep(Duration::from_millis(300)).await;

    let config = MasterConfig {
        target_address: "127.0.0.1".to_string(),
        port,
        common_address: 1,
        ..Default::default()
    };
    let mut master = MasterConnection::new(config);
    master.connect().await.unwrap();
    sleep(Duration::from_millis(500)).await;

    master.send_interrogation(1).await.unwrap();
    sleep(Duration::from_millis(2000)).await;

    {
        let data = master.received_data.read().await;
        let map = data.ca_map(1).expect("CA=1 map should exist after GI");
        for ioa in 1u32..=10 {
            let point = map.get(ioa, AsduTypeId::MMeNc1).unwrap_or_else(|| panic!("IOA={} should exist after GI", ioa));
            assert!(
                matches!(point.value, DataPointValue::ShortFloat { .. }),
                "IOA={} should be ShortFloat, got {:?}",
                ioa,
                point.value
            );
        }
    }

    master.disconnect().await.unwrap();
    slave.stop().await.unwrap();
}

// =========================================================================
// Test: Batch add different types at same IOA range, both coexist via GI
// =========================================================================
#[tokio::test]
async fn test_batch_add_coexist_then_gi() {
    let port = free_port();

    let transport = SlaveTransportConfig {
        bind_address: "127.0.0.1".to_string(),
        port,
        ..Default::default()
    };
    let mut slave = SlaveServer::new(transport);
    let mut station = Station::new(1, "Test");
    station.batch_add_points(1, 5, AsduTypeId::MSpNa1, "SP").unwrap();
    station.batch_add_points(1, 5, AsduTypeId::MMeNc1, "FL").unwrap();
    assert_eq!(station.data_points.len(), 10); // 5 SP + 5 FL coexist
    slave.add_station(station).await.unwrap();
    slave.start().await.unwrap();
    sleep(Duration::from_millis(300)).await;

    let config = MasterConfig {
        target_address: "127.0.0.1".to_string(),
        port,
        common_address: 1,
        ..Default::default()
    };
    let mut master = MasterConnection::new(config);
    master.connect().await.unwrap();
    sleep(Duration::from_millis(500)).await;

    master.send_interrogation(1).await.unwrap();
    sleep(Duration::from_millis(2000)).await;

    // Master should have received both types for IOA 1-5
    {
        let data = master.received_data.read().await;
        let map = data.ca_map(1).expect("CA=1 map should exist after GI");
        for ioa in 1u32..=5 {
            assert!(map.get(ioa, AsduTypeId::MSpNa1).is_some(), "IOA={} SP should exist after GI", ioa);
            assert!(map.get(ioa, AsduTypeId::MMeNc1).is_some(), "IOA={} FL should exist after GI", ioa);
        }
    }

    master.disconnect().await.unwrap();
    slave.stop().await.unwrap();
}

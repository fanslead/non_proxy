use super::*;

#[test]
fn decodes_bounded_ipv4_batch_and_encodes_response() {
    let record = datagram_record(AF_INET, &[192, 0, 2, 10], &[198, 51, 100, 7]);
    let mut batch = vec![0_u8; BATCH_HEADER_SIZE];
    batch.extend_from_slice(&record);
    let batch_length = u32::try_from(batch.len()).unwrap_or_default();
    write_u32(&mut batch, 0, BATCH_MAGIC);
    write_u16(&mut batch, 4, UDP_ABI_VERSION);
    write_u16(&mut batch, 6, BATCH_HEADER_SIZE as u16);
    write_u32(&mut batch, 8, batch_length);
    write_u32(&mut batch, 12, 1);

    let decoded = decode_udp_batch(&batch).unwrap_or_default();
    assert_eq!(decoded.len(), 1);
    assert_eq!(decoded[0].local().to_string(), "192.0.2.10:53000");
    assert_eq!(decoded[0].remote().to_string(), "198.51.100.7:443");
    assert_eq!(decoded[0].payload(), b"quic");
    let injection =
        UdpInjection::encode(decoded[0].injection_context(), b"reply").unwrap_or_else(|error| {
            panic!("UDP 注入编码失败: {error}");
        });
    assert!(matches!(
        read_u32(injection.as_bytes(), 0),
        Ok(INJECTION_MAGIC)
    ));
    assert_eq!(&injection.as_bytes()[INJECTION_HEADER_SIZE..], b"reply");
    let empty = UdpInjection::encode(decoded[0].injection_context(), &[])
        .unwrap_or_else(|error| panic!("空 UDP 注入编码失败: {error}"));
    assert_eq!(empty.as_bytes().len(), INJECTION_HEADER_SIZE);
}

#[test]
fn rejects_count_length_and_payload_inconsistency() {
    let mut batch = vec![0_u8; BATCH_HEADER_SIZE];
    write_u32(&mut batch, 0, BATCH_MAGIC);
    write_u16(&mut batch, 4, UDP_ABI_VERSION);
    write_u16(&mut batch, 6, BATCH_HEADER_SIZE as u16);
    write_u32(&mut batch, 8, BATCH_HEADER_SIZE as u32);
    write_u32(&mut batch, 12, 1);
    assert!(decode_udp_batch(&batch).is_err());
}

fn datagram_record(family: u16, local: &[u8], remote: &[u8]) -> Vec<u8> {
    let app = b"a\0p\0p\0";
    let payload = b"quic";
    let total = DATAGRAM_HEADER_SIZE + app.len() + payload.len();
    let mut record = vec![0_u8; total];
    write_u32(&mut record, 0, DATAGRAM_MAGIC);
    write_u16(&mut record, 4, UDP_ABI_VERSION);
    write_u16(&mut record, 6, DATAGRAM_HEADER_SIZE as u16);
    write_u32(&mut record, 8, u32::try_from(total).unwrap_or_default());
    write_u16(&mut record, 12, family);
    write_u64(&mut record, 16, 7);
    write_u64(&mut record, 24, 42);
    write_u32(&mut record, 32, 1);
    write_u32(&mut record, 36, 9);
    record[44..46].copy_from_slice(&53000_u16.to_be_bytes());
    record[46..48].copy_from_slice(&443_u16.to_be_bytes());
    record[48..48 + local.len()].copy_from_slice(local);
    record[64..64 + remote.len()].copy_from_slice(remote);
    write_u32(
        &mut record,
        80,
        u32::try_from(app.len()).unwrap_or_default(),
    );
    write_u32(
        &mut record,
        84,
        u32::try_from(payload.len()).unwrap_or_default(),
    );
    record[DATAGRAM_HEADER_SIZE..DATAGRAM_HEADER_SIZE + app.len()].copy_from_slice(app);
    record[DATAGRAM_HEADER_SIZE + app.len()..].copy_from_slice(payload);
    record
}

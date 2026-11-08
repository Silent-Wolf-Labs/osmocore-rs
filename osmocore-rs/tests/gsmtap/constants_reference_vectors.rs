use crate::gsmtap_constants_fixture;
use osmocore_rs::gsmtap::{GSMTAP_TYPE_UM, GSMTAP_UDP_PORT, GSMTAP_VERSION, channel, message_type};

const FIXTURE: &str =
    include_str!("../compatibility/vectors/gsmtap_constants/gsmtap_constants.ndjson");

/// Each of C's constants with the Rust constant for it, in the order C records them.
fn rust_constants() -> Vec<(&'static str, u16)> {
    use channel::*;
    use message_type::*;
    vec![
        ("GSMTAP_VERSION", GSMTAP_VERSION.into()),
        ("GSMTAP_UDP_PORT", GSMTAP_UDP_PORT),
        ("GSMTAP_TYPE_UM", UM.into()),
        ("GSMTAP_TYPE_ABIS", ABIS.into()),
        ("GSMTAP_TYPE_UM_BURST", UM_BURST.into()),
        ("GSMTAP_TYPE_SIM", SIM.into()),
        ("GSMTAP_TYPE_TETRA_I1", TETRA_I1.into()),
        ("GSMTAP_TYPE_TETRA_I1_BURST", TETRA_I1_BURST.into()),
        ("GSMTAP_TYPE_WMX_BURST", WMX_BURST.into()),
        ("GSMTAP_TYPE_GB_LLC", GB_LLC.into()),
        ("GSMTAP_TYPE_GB_SNDCP", GB_SNDCP.into()),
        ("GSMTAP_TYPE_GMR1_UM", GMR1_UM.into()),
        ("GSMTAP_TYPE_UMTS_RLC_MAC", UMTS_RLC_MAC.into()),
        ("GSMTAP_TYPE_UMTS_RRC", UMTS_RRC.into()),
        ("GSMTAP_TYPE_LTE_RRC", LTE_RRC.into()),
        ("GSMTAP_TYPE_LTE_MAC", LTE_MAC.into()),
        ("GSMTAP_TYPE_LTE_MAC_FRAMED", LTE_MAC_FRAMED.into()),
        ("GSMTAP_TYPE_OSMOCORE_LOG", OSMOCORE_LOG.into()),
        ("GSMTAP_TYPE_QC_DIAG", QC_DIAG.into()),
        ("GSMTAP_TYPE_LTE_NAS", LTE_NAS.into()),
        ("GSMTAP_TYPE_E1T1", E1T1.into()),
        ("GSMTAP_TYPE_GSM_RLP", GSM_RLP.into()),
        ("GSMTAP_CHANNEL_UNKNOWN", UNKNOWN.into()),
        ("GSMTAP_CHANNEL_BCCH", BCCH.into()),
        ("GSMTAP_CHANNEL_CCCH", CCCH.into()),
        ("GSMTAP_CHANNEL_RACH", RACH.into()),
        ("GSMTAP_CHANNEL_AGCH", AGCH.into()),
        ("GSMTAP_CHANNEL_PCH", PCH.into()),
        ("GSMTAP_CHANNEL_SDCCH", SDCCH.into()),
        ("GSMTAP_CHANNEL_SDCCH4", SDCCH4.into()),
        ("GSMTAP_CHANNEL_SDCCH8", SDCCH8.into()),
        ("GSMTAP_CHANNEL_FACCH_F", FACCH_F.into()),
        ("GSMTAP_CHANNEL_FACCH_H", FACCH_H.into()),
        ("GSMTAP_CHANNEL_PACCH", PACCH.into()),
        ("GSMTAP_CHANNEL_CBCH52", CBCH52.into()),
        ("GSMTAP_CHANNEL_PDTCH", PDTCH.into()),
        ("GSMTAP_CHANNEL_PDCH", PDCH.into()),
        ("GSMTAP_CHANNEL_PTCCH", PTCCH.into()),
        ("GSMTAP_CHANNEL_CBCH51", CBCH51.into()),
        ("GSMTAP_CHANNEL_VOICE_F", VOICE_F.into()),
        ("GSMTAP_CHANNEL_VOICE_H", VOICE_H.into()),
        ("GSMTAP_CHANNEL_TCH_F", TCH_F.into()),
        ("GSMTAP_CHANNEL_TCH_H", TCH_H.into()),
        ("GSMTAP_CHANNEL_ACCH", ACCH.into()),
    ]
}

#[test]
fn fixture_is_well_formed() {
    gsmtap_constants_fixture::check_corruptions(FIXTURE);
}

#[test]
fn constants_match_c() {
    let recorded = gsmtap_constants_fixture::validate(FIXTURE);
    let rust: Vec<(String, u16)> = rust_constants()
        .into_iter()
        .map(|(name, value)| (name.to_owned(), value))
        .collect();
    assert_eq!(rust, recorded);
    assert_eq!(GSMTAP_TYPE_UM, message_type::UM);
}

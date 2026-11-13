//! TUAK behaviour the C observations don't pin down.

#[cfg(feature = "auth")]
mod with_auth {
    use osmocore_rs::compat::auth::{self as compat, OsmoAuthVector, OsmoSubAuthData2};
    use osmocore_rs::gsm::auth::{
        Algorithm, AuthError, K, Op, Opc, OperatorKey, ResLength, UmtsAlgorithm, UmtsSubscriber,
    };
    use osmocore_rs::gsm::tuak::{Tuak, topc_from_top};
    use std::num::NonZeroU32;

    #[test]
    fn tuak_maps_to_its_c_algorithm() {
        assert_eq!(Algorithm::from(UmtsAlgorithm::Tuak), Algorithm::Tuak);
        assert_eq!(Algorithm::Tuak.code(), compat::OSMO_AUTH_ALG_TUAK as u8);
    }

    #[test]
    fn tuak_needs_a_32_byte_operator_key() {
        let k = K::new(&[1; 16]).unwrap();
        for operator in [
            OperatorKey::Opc(Opc::new(&[2; 16]).unwrap()),
            OperatorKey::Op(Op::new(&[2; 16]).unwrap()),
        ] {
            let mut subscriber = UmtsSubscriber::new(UmtsAlgorithm::Tuak, k.clone(), operator);
            assert_eq!(
                subscriber.vector(&[0; 16], ResLength::Eight),
                Err(AuthError::OperatorKeyLength)
            );
            assert_eq!(subscriber.sqn, 0);
        }
    }

    #[test]
    fn milenage_has_no_16_byte_res() {
        let mut subscriber = UmtsSubscriber::new(
            UmtsAlgorithm::Milenage,
            K::new(&[1; 16]).unwrap(),
            OperatorKey::Opc(Opc::new(&[2; 16]).unwrap()),
        );
        let error = subscriber.vector(&[0; 16], ResLength::Sixteen).unwrap_err();
        assert_eq!(error, AuthError::ResLength(ResLength::Sixteen));
        assert_eq!(subscriber.sqn, 0);
        assert_eq!(error.to_string(), "the algorithm has no 16-byte RES");
    }

    #[test]
    fn op_gives_the_same_vector_as_its_topc() {
        let k = K::new(&[3; 32]).unwrap();
        let topc = topc_from_top(&k, &[4; 32], NonZeroU32::MIN);
        let mut with_op = UmtsSubscriber::new(
            UmtsAlgorithm::Tuak,
            k.clone(),
            OperatorKey::Op(Op::new(&[4; 32]).unwrap()),
        );
        let mut with_topc = UmtsSubscriber::new(
            UmtsAlgorithm::Tuak,
            k,
            OperatorKey::Opc(Opc::new(&topc).unwrap()),
        );
        assert_eq!(
            with_op.vector(&[5; 16], ResLength::Sixteen),
            with_topc.vector(&[5; 16], ResLength::Sixteen)
        );
    }

    #[test]
    fn more_iterations_change_every_output() {
        let tuak = Tuak::with_top(&K::new(&[6; 16]).unwrap(), &[7; 32]);
        let twice = tuak.clone().with_iterations(NonZeroU32::new(2).unwrap());
        assert_ne!(
            tuak.f1::<8>(&[0; 16], &[0; 6], &[0; 2]),
            twice.f1::<8>(&[0; 16], &[0; 6], &[0; 2])
        );
        assert_ne!(tuak.f5_star(&[0; 16]), twice.f5_star(&[0; 16]));
    }

    #[test]
    fn compat_resync_passes_with_auts_from_the_whole_key_buffer() {
        // The C behaviour the preferred API leaves out: for a 16-byte K, compat checks
        // AUTS with all 32 bytes of the buffer.
        let mut key = [0; 32];
        key[..16].fill(8);
        key[16..].fill(9);
        let topc = [10; 32];
        let auts = Tuak::new(&K::new(&key).unwrap(), &topc).auts(&[11; 16], &[0, 0, 0, 0, 0, 40]);
        let mut aud = OsmoSubAuthData2 {
            auth_type: compat::OSMO_AUTH_TYPE_UMTS,
            algo: compat::OSMO_AUTH_ALG_TUAK,
            ..OsmoSubAuthData2::default()
        };
        aud.umts.k = key;
        aud.umts.k_len = 16;
        aud.umts.opc = topc;
        aud.umts.opc_len = 32;
        let mut vec = OsmoAuthVector {
            res_len: 8,
            ..OsmoAuthVector::default()
        };
        let rc = compat::osmo_auth_gen_vec_auts2(&mut vec, &mut aud, &auts, &[11; 16], &[12; 16]);
        assert_eq!(rc, 0);
        assert_eq!((aud.umts.sqn_ms, aud.umts.sqn), (40, 41));
    }
}

#[cfg(not(feature = "auth"))]
#[test]
fn compat_reports_tuak_as_missing_without_the_feature() {
    use osmocore_rs::compat::auth::{self as compat, OsmoAuthVector, OsmoSubAuthData2};
    let mut aud = OsmoSubAuthData2 {
        auth_type: compat::OSMO_AUTH_TYPE_UMTS,
        algo: compat::OSMO_AUTH_ALG_TUAK,
        ..OsmoSubAuthData2::default()
    };
    let mut vec = OsmoAuthVector::default();
    assert_eq!(compat::osmo_auth_gen_vec2(&mut vec, &mut aud, &[0; 16]), -2);
    assert_eq!(
        compat::osmo_auth_gen_vec_auts2(&mut vec, &mut aud, &[0; 14], &[0; 16], &[0; 16]),
        -2
    );
    assert_eq!(vec, OsmoAuthVector::default());
}

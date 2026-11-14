use osmocore_rs::gsm::a5;
use osmocore_rs::gsm::auth::{
    Ck, Ik, K, Opc, OperatorKey, ResLength, UmtsAlgorithm, UmtsSubscriber,
};
use osmocore_rs::gsm::kdf::{self, NasKey};

#[test]
fn nas_key_values_are_the_type_distinguishers() {
    assert_eq!(NasKey::Encryption as u8, 1);
    assert_eq!(NasKey::Integrity as u8, 2);
    assert_eq!(NasKey::Encryption.to_string(), "NAS encryption");
}

#[test]
fn keys_hide_their_bytes() {
    let ck = Ck::new([0x11; 16]);
    let ik = Ik::new([0x22; 16]);
    let kasme = kdf::kasme(&ck, &ik, &[0x00, 0xf1, 0x10], &[0; 6]);
    let kenb = kdf::kenb(&kasme, 0);
    assert_eq!(format!("{:?}", kdf::kc128(&ck, &ik)), "Kc128(..)");
    assert_eq!(format!("{kasme:?}"), "Kasme(..)");
    assert_eq!(format!("{kenb:?}"), "Kenb(..)");
    assert_eq!(format!("{:?}", kdf::nh(&kasme, kenb.as_bytes())), "Nh(..)");
}

#[test]
fn a_vector_gives_the_lte_keys() {
    let mut subscriber = UmtsSubscriber::new(
        UmtsAlgorithm::Milenage,
        K::new(&[0x46; 16]).unwrap(),
        OperatorKey::Opc(Opc::new(&[0xcd; 16]).unwrap()),
    );
    subscriber.amf = [0x80, 0x00];
    let vector = subscriber
        .vector(&[0x23; 16], ResLength::Eight)
        .expect("a vector");
    // AUTN starts with SQN xor AK, which is what K_ASME takes.
    let concealed = vector.autn[..6].try_into().unwrap();
    let kasme = kdf::kasme(&vector.ck, &vector.ik, &[0x62, 0xf2, 0x24], &concealed);
    // Another network, or another vector, gives another K_ASME.
    assert_ne!(
        kasme,
        kdf::kasme(&vector.ck, &vector.ik, &[0x00, 0xf1, 0x10], &concealed)
    );
    let next = subscriber
        .vector(&[0x23; 16], ResLength::Eight)
        .expect("a vector");
    let concealed_next = next.autn[..6].try_into().unwrap();
    assert_ne!(
        kasme,
        kdf::kasme(&next.ck, &next.ik, &[0x62, 0xf2, 0x24], &concealed_next)
    );
    // The encryption and integrity keys of one algorithm differ.
    assert_ne!(
        kdf::knas(&kasme, NasKey::Encryption, 2),
        kdf::knas(&kasme, NasKey::Integrity, 2)
    );
}

#[test]
fn kc128_keys_a5_4() {
    let kc128 = kdf::kc128(&Ck::new([0x5a; 16]), &Ik::new([0xa5; 16]));
    let stream = a5::a5_4(kc128.as_bytes(), 0);
    let other = a5::a5_4(&[0x5a; 16], 0);
    assert_ne!(stream, other);
}

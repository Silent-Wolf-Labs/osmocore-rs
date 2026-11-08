// SPDX-License-Identifier: GPL-3.0-or-later

//! The GSM 7-bit default alphabet and septet packing for SMS and USSD.

/// The septet that marks the next one as an extension character.
const ESCAPE_SEPTET: u8 = 0x1b;
/// What a byte with no septet maps to. Packing keeps its low seven bits, 0x7f.
const NO_SEPTET: u8 = 0xff;
/// USSD pads a message with this character.
const CR: u8 = b'\r';
/// Where the extension characters start in [`SEPTET_OF_BYTE`].
const EXTENSION_OFFSET: usize = 0x7f;

/// Bytes with a septet other than their own value, from the default alphabet.
const RECODED_BYTES: [(u8, u8); 29] = [
    (b'$', 0x02),
    (b'@', 0x00),
    (b'_', 0x11),
    (0xa1, 0x40),
    (0xa3, 0x01),
    (0xa5, 0x03),
    (0xc4, 0x5b),
    (0xc5, 0x0e),
    (0xc6, 0x1c),
    (0xc7, 0x09),
    (0xc9, 0x1f),
    (0xd1, 0x5d),
    (0xd6, 0x5c),
    (0xd8, 0x0b),
    (0xdc, 0x5e),
    (0xdf, 0x1e),
    (0xe0, 0x7f),
    (0xe4, 0x7b),
    (0xe5, 0x0f),
    (0xe6, 0x1d),
    (0xe8, 0x04),
    (0xe9, 0x05),
    (0xec, 0x07),
    (0xf1, 0x7d),
    (0xf2, 0x08),
    (0xf6, 0x7c),
    (0xf8, 0x0c),
    (0xf9, 0x06),
    (0xfc, 0x7e),
];

/// Extension characters as (septet after the escape, byte), and the septet each
/// byte encodes to where that isn't the same. Form feed has no septet of its own.
const EXTENSION_CHARACTERS: [(u8, u8, u8); 9] = [
    (0x0a, 0x0c, NO_SEPTET),
    (0x14, b'^', 0x14),
    (0x28, b'{', 0x28),
    (0x29, b'}', 0x29),
    (0x2f, b'\\', 0x2f),
    (0x3c, b'[', 0x3c),
    (0x3d, b'~', 0x3d),
    (0x3e, b']', 0x3e),
    (0x40, b'|', 0x40),
];

const fn identity_byte(byte: u8) -> bool {
    matches!(byte, b'\n' | b'\r' | b' '..=b'#' | b'%'..=b'?' | b'A'..=b'Z' | b'a'..=b'z')
}

/// The septet of each byte. As in C, one table also holds the extension characters'
/// bytes from [`EXTENSION_OFFSET`], so those positions replace the septets of the
/// bytes that land there. That is why 0xa7 encodes to 0x7b, not to the 0x5f of `§`.
const SEPTET_OF_BYTE: [u8; 256] = {
    let mut table = [NO_SEPTET; 256];
    let mut byte = 0;
    while byte < 256 {
        if identity_byte(byte as u8) {
            table[byte] = byte as u8;
        }
        byte += 1;
    }
    let mut index = 0;
    while index < RECODED_BYTES.len() {
        let (byte, septet) = RECODED_BYTES[index];
        table[byte as usize] = septet;
        index += 1;
    }
    let mut index = 0;
    while index < EXTENSION_CHARACTERS.len() {
        let (extension, byte, septet) = EXTENSION_CHARACTERS[index];
        table[byte as usize] = septet;
        table[EXTENSION_OFFSET + extension as usize] = byte;
        index += 1;
    }
    table
};

/// The byte of each septet: the lowest byte with that entry in [`SEPTET_OF_BYTE`], or
/// 0xff. The shared table means that for some septets this is an extension position.
const BYTE_OF_SEPTET: [u8; 128] = {
    let mut table = [0xff; 128];
    let mut byte = 256;
    while byte > 0 {
        byte -= 1;
        let septet = SEPTET_OF_BYTE[byte];
        if septet < 128 {
            table[septet as usize] = byte as u8;
        }
    }
    table
};

const NEEDS_ESCAPE: [bool; 256] = {
    let mut table = [false; 256];
    let mut index = 0;
    while index < EXTENSION_CHARACTERS.len() {
        table[EXTENSION_CHARACTERS[index].1 as usize] = true;
        index += 1;
    }
    table
};

/// Text packed into the 7-bit default alphabet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Packed7bit {
    /// The packed octets.
    pub octets: Vec<u8>,
    /// The septets they hold. The octet count alone can't give this: 7 and 8 septets
    /// both take 7 octets.
    pub septets: usize,
}

/// Returns the octets needed to pack `septets` septets.
pub const fn septets_to_octets(septets: usize) -> usize {
    septets / 8 * 7 + (septets % 8 * 7).div_ceil(8)
}

/// Converts bytes to unpacked septets, putting an escape before each extension
/// character. A byte with no septet, including NUL, gives 0xff.
pub fn encode_septets(text: &[u8]) -> Vec<u8> {
    let mut septets = Vec::with_capacity(text.len());
    for &byte in text {
        if NEEDS_ESCAPE[usize::from(byte)] {
            septets.push(ESCAPE_SEPTET);
        }
        septets.push(SEPTET_OF_BYTE[usize::from(byte)]);
    }
    septets
}

/// Packs septets into octets after `padding` zero bits, using each one's low seven
/// bits. Returns `None` if `padding` is above 7.
pub fn pack_septets(septets: &[u8], padding: u8) -> Option<Vec<u8>> {
    if padding > 7 {
        return None;
    }
    let mut octets = Vec::with_capacity(septets_to_octets(septets.len()) + 1);
    let mut pending = 0u16;
    let mut bits = u32::from(padding);
    for septet in septets {
        pending |= u16::from(septet & 0x7f) << bits;
        bits += 7;
        if bits >= 8 {
            octets.push(pending as u8);
            pending >>= 8;
            bits -= 8;
        }
    }
    if bits > 0 {
        octets.push(pending as u8);
    }
    Some(octets)
}

/// Encodes and packs text, keeping as many septets as fit in `max_octets`.
///
/// Text is bytes, not UTF-8: the alphabet maps Latin-1 values. A limit can leave an
/// escape as the last septet. A byte with no septet packs as 0x7f.
pub fn encode_7bit(text: &[u8], max_octets: usize) -> Packed7bit {
    let mut septets = encode_septets(text);
    septets.truncate(max_octets.saturating_mul(8) / 7);
    let octets = pack_septets(&septets, 0).expect("zero padding is valid");
    Packed7bit {
        octets,
        septets: septets.len(),
    }
}

/// Encodes text for USSD. Like [`encode_7bit`], then fills a last octet with one spare
/// septet with `<CR>`, so that it isn't read as an `@`. A last septet of `<CR>` that
/// ends exactly on an octet gets a second `<CR>`, unless that would use the last octet
/// allowed, even though it would fit.
pub fn encode_7bit_ussd(text: &[u8], max_octets: usize) -> Packed7bit {
    let mut packed = encode_7bit(text, max_octets);
    let room_for_more = packed.octets.len() + 1 < max_octets;
    match (packed.septets % 8, packed.octets.last_mut()) {
        (7, Some(last)) => *last |= CR << 1,
        (0, Some(last)) if *last >> 1 == CR && room_for_more => {
            packed.octets.push(CR);
            packed.septets += 1;
        }
        _ => {}
    }
    packed
}

/// Reads `count` septets starting `first` septets in, stopping after `limit` bytes of
/// output. Octets at or past `octet_limit` don't add to a septet that straddles them,
/// and octets past the end of `packed` read as zero.
pub(crate) fn unpack_text(
    packed: &[u8],
    first: usize,
    count: usize,
    octet_limit: usize,
    limit: usize,
) -> Vec<u8> {
    let octet = |index: usize| packed.get(index).copied().unwrap_or(0);
    let mut text = Vec::with_capacity(count.min(limit));
    let mut escaped = false;
    for index in 0..count {
        if text.len() == limit {
            break;
        }
        let bit = (first + index) * 7;
        let (low, shift) = (bit / 8, bit % 8);
        let high = if shift > 1 && low + 1 < octet_limit {
            octet(low + 1) << (8 - shift)
        } else {
            0
        };
        let septet = ((octet(low) >> shift) | high) & 0x7f;
        if escaped {
            escaped = false;
            text.push(SEPTET_OF_BYTE[EXTENSION_OFFSET + usize::from(septet)]);
        } else if septet == ESCAPE_SEPTET && index + 1 < count {
            escaped = true;
        } else {
            text.push(BYTE_OF_SEPTET[usize::from(septet)]);
        }
    }
    text
}

/// Returns the septets taken by a user data header whose length octet is `length`.
pub(crate) const fn header_septets(length: u8) -> usize {
    ((length as usize + 1) * 8).div_ceil(7)
}

/// Decodes `septets` packed septets to bytes. Returns `None` if `packed` is too short
/// to hold them.
///
/// The output is bytes, not UTF-8. Septets with no byte give 0xff, and some accented
/// letters give C's values rather than their Latin-1 ones; see the source guide.
pub fn decode_7bit(packed: &[u8], septets: usize) -> Option<Vec<u8>> {
    let octets = septets_to_octets(septets);
    (packed.len() >= octets).then(|| unpack_text(packed, 0, septets, octets, usize::MAX))
}

/// Decodes a message that starts with a user data header, returning the text after
/// it. `septets` counts the header too. Returns `None` if `packed` is too short or the
/// header doesn't fit in `septets`.
pub fn decode_7bit_with_header(packed: &[u8], septets: usize) -> Option<Vec<u8>> {
    let octets = septets_to_octets(septets);
    let first = header_septets(*packed.first()?);
    (packed.len() >= octets && first <= septets)
        .then(|| unpack_text(packed, first, septets - first, octets, usize::MAX))
}

/// Decodes a USSD message: like [`decode_7bit`], then drops the last byte if the top
/// seven bits of the last octet hold `<CR>`, wherever the last septet ends.
pub fn decode_7bit_ussd(packed: &[u8], septets: usize) -> Option<Vec<u8>> {
    let mut text = decode_7bit(packed, septets)?;
    let octets = septets_to_octets(septets);
    if !text.is_empty() && packed[octets - 1] >> 1 == CR {
        text.pop();
    }
    Some(text)
}

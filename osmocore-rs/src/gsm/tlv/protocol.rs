// SPDX-License-Identifier: GPL-3.0-or-later

//! Protocol definitions and their message validation.

use super::*;

/// What a protocol requires of one message type.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TlvMessageDef<'a> {
    pub name: Option<&'a str>,
    /// Tags this message must have.
    pub mandatory: &'a [u8],
    /// Free for the protocol's own use, such as direction.
    pub flags: u32,
}

/// What a protocol requires of one tag.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TlvIeDef<'a> {
    pub min_len: u16,
    pub name: Option<&'a str>,
}

/// A protocol's message and element rules, on top of a [`TlvDefinition`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TlvProtocol<'a> {
    pub name: &'a str,
    pub definition: &'a TlvDefinition,
    /// Names for message types with no [`TlvMessageDef::name`].
    pub message_names: Option<&'a [(u8, &'a str)]>,
    messages: [TlvMessageDef<'a>; 256],
    ies: [TlvIeDef<'a>; 256],
}

impl<'a> TlvProtocol<'a> {
    pub fn new(name: &'a str, definition: &'a TlvDefinition) -> Self {
        Self {
            name,
            definition,
            message_names: None,
            messages: [TlvMessageDef {
                name: None,
                mandatory: &[],
                flags: 0,
            }; 256],
            ies: [TlvIeDef {
                min_len: 0,
                name: None,
            }; 256],
        }
    }
    pub fn set_message(&mut self, message_type: u8, def: TlvMessageDef<'a>) {
        self.messages[usize::from(message_type)] = def;
    }
    pub fn set_ie(&mut self, iei: u8, def: TlvIeDef<'a>) {
        self.ies[usize::from(iei)] = def;
    }
    pub fn flags(&self, message_type: u8) -> u32 {
        self.messages[usize::from(message_type)].flags
    }
    pub fn message_name(&self, message_type: u8) -> Cow<'a, str> {
        if let Some(name) = self.messages[usize::from(message_type)].name {
            return Cow::Borrowed(name);
        }
        match self.message_names {
            Some(names) => names
                .iter()
                .find(|(value, _)| *value == message_type)
                .map_or_else(
                    || Cow::Owned(format!("unknown 0x{message_type:x}")),
                    |(_, name)| Cow::Borrowed(*name),
                ),
            None => Cow::Owned(format!("Unknown msg_type 0x{message_type:02x}")),
        }
    }
    pub fn ie_name(&self, iei: u8) -> Cow<'a, str> {
        match self.ies[usize::from(iei)].name {
            Some(name) => Cow::Borrowed(name),
            None => Cow::Owned(format!("Unknown IEI 0x{iei:02x}")),
        }
    }
    /// Check a parsed message: first that its mandatory elements are present, in the
    /// order listed, then that its elements meet their minimum lengths, in tag order.
    /// Returns the first problem.
    pub fn validate(&self, message_type: u8, parsed: &TlvParsed<'_>) -> Result<(), TlvError> {
        let message = &self.messages[usize::from(message_type)];
        if let Some(&iei) = message
            .mandatory
            .iter()
            .find(|iei| !parsed.is_present(**iei))
        {
            return Err(TlvError::MandatoryIeMissing { iei });
        }
        for ie in parsed.iter() {
            let min_len = self.ies[usize::from(ie.tag)].min_len;
            if ie.value.len() < usize::from(min_len) {
                return Err(TlvError::IeTooShort {
                    iei: ie.tag,
                    len: ie.value.len() as u16,
                    min_len,
                });
            }
        }
        Ok(())
    }
    /// Parse a message with [`parse_multiple`] and validate `decoded[0]`. With an empty
    /// `decoded`, only the mandatory and length rules for no elements apply.
    pub fn parse<'b>(
        &self,
        decoded: &mut [TlvParsed<'b>],
        message_type: u8,
        buf: &'b [u8],
        lv_tags: [Option<u8>; 2],
    ) -> Result<(), TlvError> {
        parse_multiple(decoded, self.definition, buf, lv_tags)?;
        match decoded.first() {
            Some(first) => self.validate(message_type, first),
            None => self.validate(message_type, &TlvParsed::new()),
        }
    }
}

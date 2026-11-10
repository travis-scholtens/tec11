//! Protocol Buffers (`proto3`) wire-format encoder and decoder for `proto/tec11.proto`
//! (`package tec11;`, `.tec11` binary snapshot files).

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecodeError(pub &'static str);

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}

impl std::error::Error for DecodeError {}

const WIRE_VARINT: u32 = 0;
const WIRE_I64: u32 = 1;
const WIRE_LEN: u32 = 2;
const WIRE_I32: u32 = 5;

#[inline]
fn encode_varint(mut v: u64, buf: &mut Vec<u8>) {
    while v >= 0x80 {
        buf.push((v as u8) | 0x80);
        v >>= 7;
    }
    buf.push(v as u8);
}

#[inline]
fn decode_varint(slice: &mut &[u8]) -> Result<u64, DecodeError> {
    let mut result = 0u64;
    let mut shift = 0u32;
    loop {
        if slice.is_empty() {
            return Err(DecodeError("unexpected EOF reading varint"));
        }
        let b = slice[0];
        *slice = &slice[1..];
        result |= ((b & 0x7F) as u64) << shift;
        if (b & 0x80) == 0 {
            return Ok(result);
        }
        shift += 7;
        if shift >= 64 {
            return Err(DecodeError("varint overflow"));
        }
    }
}

#[inline]
fn encode_tag(field_number: u32, wire_type: u32, buf: &mut Vec<u8>) {
    encode_varint(((field_number << 3) | wire_type) as u64, buf);
}

#[inline]
fn decode_tag(slice: &mut &[u8]) -> Result<(u32, u32), DecodeError> {
    let v = decode_varint(slice)? as u32;
    let field_number = v >> 3;
    let wire_type = v & 0x07;
    if field_number == 0 {
        return Err(DecodeError("invalid field number 0"));
    }
    Ok((field_number, wire_type))
}

#[inline]
fn encode_f32_field(field_number: u32, v: f32, buf: &mut Vec<u8>) {
    if v.to_bits() != 0 {
        encode_tag(field_number, WIRE_I32, buf);
        buf.extend_from_slice(&v.to_le_bytes());
    }
}

#[inline]
fn encode_opt_f32_field(field_number: u32, v: Option<f32>, buf: &mut Vec<u8>) {
    if let Some(val) = v {
        encode_tag(field_number, WIRE_I32, buf);
        buf.extend_from_slice(&val.to_le_bytes());
    }
}

#[inline]
fn decode_f32(slice: &mut &[u8], wire_type: u32) -> Result<f32, DecodeError> {
    if wire_type != WIRE_I32 {
        return Err(DecodeError("expected I32 wire type for float"));
    }
    if slice.len() < 4 {
        return Err(DecodeError("unexpected EOF reading float"));
    }
    let bytes = [slice[0], slice[1], slice[2], slice[3]];
    *slice = &slice[4..];
    Ok(f32::from_le_bytes(bytes))
}

#[inline]
fn encode_u32_field(field_number: u32, v: u32, buf: &mut Vec<u8>) {
    if v != 0 {
        encode_tag(field_number, WIRE_VARINT, buf);
        encode_varint(v as u64, buf);
    }
}

#[inline]
fn encode_opt_u32_field(field_number: u32, v: Option<u32>, buf: &mut Vec<u8>) {
    if let Some(val) = v {
        encode_tag(field_number, WIRE_VARINT, buf);
        encode_varint(val as u64, buf);
    }
}

#[inline]
fn decode_u32(slice: &mut &[u8], wire_type: u32) -> Result<u32, DecodeError> {
    if wire_type != WIRE_VARINT {
        return Err(DecodeError("expected VARINT wire type for uint32"));
    }
    Ok(decode_varint(slice)? as u32)
}

#[inline]
fn encode_bool_field(field_number: u32, v: bool, buf: &mut Vec<u8>) {
    if v {
        encode_tag(field_number, WIRE_VARINT, buf);
        buf.push(1);
    }
}

#[inline]
fn decode_bool(slice: &mut &[u8], wire_type: u32) -> Result<bool, DecodeError> {
    if wire_type != WIRE_VARINT {
        return Err(DecodeError("expected VARINT wire type for bool"));
    }
    Ok(decode_varint(slice)? != 0)
}

#[inline]
fn encode_bytes_field(field_number: u32, bytes: &[u8], buf: &mut Vec<u8>) {
    if !bytes.is_empty() {
        encode_tag(field_number, WIRE_LEN, buf);
        encode_varint(bytes.len() as u64, buf);
        buf.extend_from_slice(bytes);
    }
}

#[inline]
fn decode_len_slice<'a>(slice: &mut &'a [u8], wire_type: u32) -> Result<&'a [u8], DecodeError> {
    if wire_type != WIRE_LEN {
        return Err(DecodeError("expected LEN wire type"));
    }
    let len = decode_varint(slice)? as usize;
    if slice.len() < len {
        return Err(DecodeError("unexpected EOF reading length-delimited field"));
    }
    let sub = &slice[..len];
    *slice = &slice[len..];
    Ok(sub)
}

#[inline]
fn encode_packed_u32_field(field_number: u32, values: &[u32], buf: &mut Vec<u8>) {
    if values.is_empty() {
        return;
    }
    let mut inner = Vec::with_capacity(values.len() * 2);
    for &v in values {
        encode_varint(v as u64, &mut inner);
    }
    encode_tag(field_number, WIRE_LEN, buf);
    encode_varint(inner.len() as u64, buf);
    buf.extend_from_slice(&inner);
}

#[inline]
fn decode_packed_or_single_u32(
    slice: &mut &[u8],
    wire_type: u32,
    out: &mut Vec<u32>,
) -> Result<(), DecodeError> {
    if wire_type == WIRE_LEN {
        let mut sub = decode_len_slice(slice, wire_type)?;
        while !sub.is_empty() {
            out.push(decode_varint(&mut sub)? as u32);
        }
        Ok(())
    } else if wire_type == WIRE_VARINT {
        out.push(decode_varint(slice)? as u32);
        Ok(())
    } else {
        Err(DecodeError("expected LEN or VARINT for repeated uint32"))
    }
}

#[inline]
fn encode_submessage<F: FnOnce(&mut Vec<u8>)>(field_number: u32, buf: &mut Vec<u8>, f: F) {
    let mut inner = Vec::new();
    f(&mut inner);
    encode_tag(field_number, WIRE_LEN, buf);
    encode_varint(inner.len() as u64, buf);
    buf.extend_from_slice(&inner);
}

fn skip_field(slice: &mut &[u8], wire_type: u32) -> Result<(), DecodeError> {
    match wire_type {
        WIRE_VARINT => {
            let _ = decode_varint(slice)?;
            Ok(())
        }
        WIRE_I64 => {
            if slice.len() < 8 {
                return Err(DecodeError("unexpected EOF skipping I64"));
            }
            *slice = &slice[8..];
            Ok(())
        }
        WIRE_LEN => {
            let _ = decode_len_slice(slice, wire_type)?;
            Ok(())
        }
        WIRE_I32 => {
            if slice.len() < 4 {
                return Err(DecodeError("unexpected EOF skipping I32"));
            }
            *slice = &slice[4..];
            Ok(())
        }
        _ => Err(DecodeError("unsupported wire type")),
    }
}

// ============================================================================
// Protobuf Messages (`proto/tec11.proto`)
// ============================================================================

#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    pub fn encode(&self, buf: &mut Vec<u8>) {
        encode_f32_field(1, self.x, buf);
        encode_f32_field(2, self.y, buf);
        encode_f32_field(3, self.z, buf);
    }

    pub fn decode(mut slice: &[u8]) -> Result<Self, DecodeError> {
        let mut msg = Self::default();
        while !slice.is_empty() {
            let (tag, wt) = decode_tag(&mut slice)?;
            match tag {
                1 => msg.x = decode_f32(&mut slice, wt)?,
                2 => msg.y = decode_f32(&mut slice, wt)?,
                3 => msg.z = decode_f32(&mut slice, wt)?,
                _ => skip_field(&mut slice, wt)?,
            }
        }
        Ok(msg)
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct IgneousRock {
    pub is_intrusive: bool,
    pub felsity: f32,
}

impl IgneousRock {
    pub fn encode(&self, buf: &mut Vec<u8>) {
        encode_bool_field(1, self.is_intrusive, buf);
        encode_f32_field(2, self.felsity, buf);
    }

    pub fn decode(mut slice: &[u8]) -> Result<Self, DecodeError> {
        let mut msg = Self::default();
        while !slice.is_empty() {
            let (tag, wt) = decode_tag(&mut slice)?;
            match tag {
                1 => msg.is_intrusive = decode_bool(&mut slice, wt)?,
                2 => msg.felsity = decode_f32(&mut slice, wt)?,
                _ => skip_field(&mut slice, wt)?,
            }
        }
        Ok(msg)
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct SedimentaryRock {
    pub clasticity: f32,
    pub felsity: Option<f32>,
    pub calcity: Option<f32>,
    pub bogginess: Option<f32>,
}

impl SedimentaryRock {
    pub fn encode(&self, buf: &mut Vec<u8>) {
        encode_f32_field(1, self.clasticity, buf);
        encode_opt_f32_field(2, self.felsity, buf);
        encode_opt_f32_field(3, self.calcity, buf);
        encode_opt_f32_field(4, self.bogginess, buf);
    }

    pub fn decode(mut slice: &[u8]) -> Result<Self, DecodeError> {
        let mut msg = Self::default();
        while !slice.is_empty() {
            let (tag, wt) = decode_tag(&mut slice)?;
            match tag {
                1 => msg.clasticity = decode_f32(&mut slice, wt)?,
                2 => msg.felsity = Some(decode_f32(&mut slice, wt)?),
                3 => msg.calcity = Some(decode_f32(&mut slice, wt)?),
                4 => msg.bogginess = Some(decode_f32(&mut slice, wt)?),
                _ => skip_field(&mut slice, wt)?,
            }
        }
        Ok(msg)
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
#[repr(u32)]
pub enum MetamorphicFacies {
    #[default]
    Pelite = 0,
    Psammite = 1,
    Psephite = 2,
    Blueschist = 3,
    Greenschist = 4,
    Amphibolite = 5,
    Eclogite = 6,
    Hornfels = 7,
    Granulite = 8,
    Marble = 9,
}

impl MetamorphicFacies {
    pub fn from_u32(v: u32) -> Self {
        match v {
            0 => Self::Pelite,
            1 => Self::Psammite,
            2 => Self::Psephite,
            3 => Self::Blueschist,
            4 => Self::Greenschist,
            5 => Self::Amphibolite,
            6 => Self::Eclogite,
            7 => Self::Hornfels,
            8 => Self::Granulite,
            9 => Self::Marble,
            _ => Self::Pelite,
        }
    }

    pub fn rock_name(self) -> &'static str {
        match self {
            Self::Pelite => "pelite",
            Self::Psammite => "psammite",
            Self::Psephite => "psephite",
            Self::Blueschist => "blueschist",
            Self::Greenschist => "greenschist",
            Self::Amphibolite => "amphibolite",
            Self::Eclogite => "eclogite",
            Self::Hornfels => "hornfels",
            Self::Granulite => "granulite",
            Self::Marble => "marble",
        }
    }

    pub fn from_rock_name(name: &str) -> Self {
        match name {
            "pelite" => Self::Pelite,
            "psammite" => Self::Psammite,
            "psephite" => Self::Psephite,
            "blueschist" => Self::Blueschist,
            "greenschist" => Self::Greenschist,
            "amphibolite" => Self::Amphibolite,
            "eclogite" => Self::Eclogite,
            "hornfels" => Self::Hornfels,
            "granulite" => Self::Granulite,
            "marble" => Self::Marble,
            _ => Self::Pelite,
        }
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct MetamorphicRock {
    pub facies: MetamorphicFacies,
    pub toughness: f32,
    pub felsity: Option<f32>,
    pub calcity: Option<f32>,
    pub bogginess: Option<f32>,
}

impl MetamorphicRock {
    pub fn encode(&self, buf: &mut Vec<u8>) {
        encode_u32_field(1, self.facies as u32, buf);
        encode_f32_field(2, self.toughness, buf);
        encode_opt_f32_field(3, self.felsity, buf);
        encode_opt_f32_field(4, self.calcity, buf);
        encode_opt_f32_field(5, self.bogginess, buf);
    }

    pub fn decode(mut slice: &[u8]) -> Result<Self, DecodeError> {
        let mut msg = Self::default();
        while !slice.is_empty() {
            let (tag, wt) = decode_tag(&mut slice)?;
            match tag {
                1 => msg.facies = MetamorphicFacies::from_u32(decode_u32(&mut slice, wt)?),
                2 => msg.toughness = decode_f32(&mut slice, wt)?,
                3 => msg.felsity = Some(decode_f32(&mut slice, wt)?),
                4 => msg.calcity = Some(decode_f32(&mut slice, wt)?),
                5 => msg.bogginess = Some(decode_f32(&mut slice, wt)?),
                _ => skip_field(&mut slice, wt)?,
            }
        }
        Ok(msg)
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum RockKindOneof {
    Igneous(IgneousRock),
    Sedimentary(SedimentaryRock),
    Metamorphic(MetamorphicRock),
}

#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct Rock {
    pub kind: Option<RockKindOneof>,
}

impl Rock {
    pub fn encode(&self, buf: &mut Vec<u8>) {
        if let Some(ref k) = self.kind {
            match k {
                RockKindOneof::Igneous(ig) => encode_submessage(1, buf, |b| ig.encode(b)),
                RockKindOneof::Sedimentary(sed) => encode_submessage(2, buf, |b| sed.encode(b)),
                RockKindOneof::Metamorphic(met) => encode_submessage(3, buf, |b| met.encode(b)),
            }
        }
    }

    pub fn decode(mut slice: &[u8]) -> Result<Self, DecodeError> {
        let mut msg = Self::default();
        while !slice.is_empty() {
            let (tag, wt) = decode_tag(&mut slice)?;
            match tag {
                1 => {
                    let sub = decode_len_slice(&mut slice, wt)?;
                    msg.kind = Some(RockKindOneof::Igneous(IgneousRock::decode(sub)?));
                }
                2 => {
                    let sub = decode_len_slice(&mut slice, wt)?;
                    msg.kind = Some(RockKindOneof::Sedimentary(SedimentaryRock::decode(sub)?));
                }
                3 => {
                    let sub = decode_len_slice(&mut slice, wt)?;
                    msg.kind = Some(RockKindOneof::Metamorphic(MetamorphicRock::decode(sub)?));
                }
                _ => skip_field(&mut slice, wt)?,
            }
        }
        Ok(msg)
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct RockLayer {
    pub rock: Option<Rock>,
    pub thickness: f32,
}

impl RockLayer {
    pub fn encode(&self, buf: &mut Vec<u8>) {
        if let Some(ref r) = self.rock {
            encode_submessage(1, buf, |b| r.encode(b));
        }
        encode_f32_field(2, self.thickness, buf);
    }

    pub fn decode(mut slice: &[u8]) -> Result<Self, DecodeError> {
        let mut msg = Self::default();
        while !slice.is_empty() {
            let (tag, wt) = decode_tag(&mut slice)?;
            match tag {
                1 => {
                    let sub = decode_len_slice(&mut slice, wt)?;
                    msg.rock = Some(Rock::decode(sub)?);
                }
                2 => msg.thickness = decode_f32(&mut slice, wt)?,
                _ => skip_field(&mut slice, wt)?,
            }
        }
        Ok(msg)
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LithosphericGroup {
    pub tile_ids: Vec<u32>,
    pub velocity: Option<Vec3>,
}

impl LithosphericGroup {
    pub fn encode(&self, buf: &mut Vec<u8>) {
        encode_packed_u32_field(1, &self.tile_ids, buf);
        if let Some(ref v) = self.velocity {
            encode_submessage(2, buf, |b| v.encode(b));
        }
    }

    pub fn decode(mut slice: &[u8]) -> Result<Self, DecodeError> {
        let mut msg = Self::default();
        while !slice.is_empty() {
            let (tag, wt) = decode_tag(&mut slice)?;
            match tag {
                1 => decode_packed_or_single_u32(&mut slice, wt, &mut msg.tile_ids)?,
                2 => {
                    let sub = decode_len_slice(&mut slice, wt)?;
                    msg.velocity = Some(Vec3::decode(sub)?);
                }
                _ => skip_field(&mut slice, wt)?,
            }
        }
        Ok(msg)
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct SpreadingRidgeArc {
    pub start: Option<Vec3>,
    pub end: Option<Vec3>,
    pub velocity: Option<Vec3>,
    pub age_ticks: u32,
}

impl SpreadingRidgeArc {
    pub fn encode(&self, buf: &mut Vec<u8>) {
        if let Some(ref s) = self.start {
            encode_submessage(1, buf, |b| s.encode(b));
        }
        if let Some(ref e) = self.end {
            encode_submessage(2, buf, |b| e.encode(b));
        }
        if let Some(ref v) = self.velocity {
            encode_submessage(3, buf, |b| v.encode(b));
        }
        encode_u32_field(4, self.age_ticks, buf);
    }

    pub fn decode(mut slice: &[u8]) -> Result<Self, DecodeError> {
        let mut msg = Self::default();
        while !slice.is_empty() {
            let (tag, wt) = decode_tag(&mut slice)?;
            match tag {
                1 => {
                    let sub = decode_len_slice(&mut slice, wt)?;
                    msg.start = Some(Vec3::decode(sub)?);
                }
                2 => {
                    let sub = decode_len_slice(&mut slice, wt)?;
                    msg.end = Some(Vec3::decode(sub)?);
                }
                3 => {
                    let sub = decode_len_slice(&mut slice, wt)?;
                    msg.velocity = Some(Vec3::decode(sub)?);
                }
                4 => msg.age_ticks = decode_u32(&mut slice, wt)?,
                _ => skip_field(&mut slice, wt)?,
            }
        }
        Ok(msg)
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct GeologicalTile {
    pub bottom: f32,
    pub mountainosity: f32,
    pub layers: Vec<RockLayer>,
}

impl GeologicalTile {
    #[inline]
    pub fn elevation(&self) -> f32 {
        let thickness: f32 = self.layers.iter().map(|l| l.thickness).sum();
        self.bottom + thickness
    }

    pub fn encode(&self, buf: &mut Vec<u8>) {
        encode_f32_field(1, self.bottom, buf);
        encode_f32_field(2, self.mountainosity, buf);
        for l in &self.layers {
            encode_submessage(3, buf, |b| l.encode(b));
        }
    }

    pub fn decode(mut slice: &[u8]) -> Result<Self, DecodeError> {
        let mut msg = Self::default();
        while !slice.is_empty() {
            let (tag, wt) = decode_tag(&mut slice)?;
            match tag {
                1 => msg.bottom = decode_f32(&mut slice, wt)?,
                2 => msg.mountainosity = decode_f32(&mut slice, wt)?,
                3 => {
                    let sub = decode_len_slice(&mut slice, wt)?;
                    msg.layers.push(RockLayer::decode(sub)?);
                }
                _ => skip_field(&mut slice, wt)?,
            }
        }
        Ok(msg)
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PrehistoryTile {
    pub geology: Option<GeologicalTile>,
    pub heritage_id: Option<u32>,
}

impl PrehistoryTile {
    pub fn encode(&self, buf: &mut Vec<u8>) {
        if let Some(ref g) = self.geology {
            encode_submessage(1, buf, |b| g.encode(b));
        }
        encode_opt_u32_field(2, self.heritage_id, buf);
    }

    pub fn decode(mut slice: &[u8]) -> Result<Self, DecodeError> {
        let mut msg = Self::default();
        while !slice.is_empty() {
            let (tag, wt) = decode_tag(&mut slice)?;
            match tag {
                1 => {
                    let sub = decode_len_slice(&mut slice, wt)?;
                    msg.geology = Some(GeologicalTile::decode(sub)?);
                }
                2 => msg.heritage_id = Some(decode_u32(&mut slice, wt)?),
                _ => skip_field(&mut slice, wt)?,
            }
        }
        Ok(msg)
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TectonicsStage {
    pub tectonics_ticks: u32,
    pub atmosphere_ticks_remaining: u32,
    pub life_ticks_remaining: u32,
    pub people_ticks_remaining: u32,
    pub tiles: Vec<GeologicalTile>,
    pub groups: Vec<LithosphericGroup>,
    pub ridges: Vec<SpreadingRidgeArc>,
}

impl TectonicsStage {
    pub fn encode(&self, buf: &mut Vec<u8>) {
        encode_u32_field(1, self.tectonics_ticks, buf);
        encode_u32_field(2, self.atmosphere_ticks_remaining, buf);
        encode_u32_field(3, self.life_ticks_remaining, buf);
        encode_u32_field(4, self.people_ticks_remaining, buf);
        for t in &self.tiles {
            encode_submessage(5, buf, |b| t.encode(b));
        }
        for g in &self.groups {
            encode_submessage(6, buf, |b| g.encode(b));
        }
        for r in &self.ridges {
            encode_submessage(7, buf, |b| r.encode(b));
        }
    }

    pub fn decode(mut slice: &[u8]) -> Result<Self, DecodeError> {
        let mut msg = Self::default();
        while !slice.is_empty() {
            let (tag, wt) = decode_tag(&mut slice)?;
            match tag {
                1 => msg.tectonics_ticks = decode_u32(&mut slice, wt)?,
                2 => msg.atmosphere_ticks_remaining = decode_u32(&mut slice, wt)?,
                3 => msg.life_ticks_remaining = decode_u32(&mut slice, wt)?,
                4 => msg.people_ticks_remaining = decode_u32(&mut slice, wt)?,
                5 => {
                    let sub = decode_len_slice(&mut slice, wt)?;
                    msg.tiles.push(GeologicalTile::decode(sub)?);
                }
                6 => {
                    let sub = decode_len_slice(&mut slice, wt)?;
                    msg.groups.push(LithosphericGroup::decode(sub)?);
                }
                7 => {
                    let sub = decode_len_slice(&mut slice, wt)?;
                    msg.ridges.push(SpreadingRidgeArc::decode(sub)?);
                }
                _ => skip_field(&mut slice, wt)?,
            }
        }
        Ok(msg)
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Heritage {
    pub name: String,
    pub parent_heritage_ids: Vec<u32>,
    pub agricultural: bool,
}

impl Heritage {
    pub fn encode(&self, buf: &mut Vec<u8>) {
        encode_bytes_field(1, self.name.as_bytes(), buf);
        encode_packed_u32_field(2, &self.parent_heritage_ids, buf);
        encode_bool_field(3, self.agricultural, buf);
    }

    pub fn decode(mut slice: &[u8]) -> Result<Self, DecodeError> {
        let mut msg = Self::default();
        while !slice.is_empty() {
            let (tag, wt) = decode_tag(&mut slice)?;
            match tag {
                1 => {
                    let sub = decode_len_slice(&mut slice, wt)?;
                    msg.name = String::from_utf8(sub.to_vec())
                        .map_err(|_| DecodeError("invalid UTF-8 in Heritage.name"))?;
                }
                2 => decode_packed_or_single_u32(&mut slice, wt, &mut msg.parent_heritage_ids)?,
                3 => msg.agricultural = decode_bool(&mut slice, wt)?,
                _ => skip_field(&mut slice, wt)?,
            }
        }
        Ok(msg)
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PrehistoryStage {
    pub tectonics_ticks: u32,
    pub prehistory_ticks: u32,
    pub glaciation_tick: u32,
    pub tiles: Vec<PrehistoryTile>,
    pub groups: Vec<LithosphericGroup>,
    pub heritages: Vec<Heritage>,
}

impl PrehistoryStage {
    pub fn encode(&self, buf: &mut Vec<u8>) {
        encode_u32_field(1, self.tectonics_ticks, buf);
        encode_u32_field(2, self.prehistory_ticks, buf);
        encode_u32_field(3, self.glaciation_tick, buf);
        for t in &self.tiles {
            encode_submessage(4, buf, |b| t.encode(b));
        }
        for g in &self.groups {
            encode_submessage(5, buf, |b| g.encode(b));
        }
        for h in &self.heritages {
            encode_submessage(6, buf, |b| h.encode(b));
        }
    }

    pub fn decode(mut slice: &[u8]) -> Result<Self, DecodeError> {
        let mut msg = Self::default();
        while !slice.is_empty() {
            let (tag, wt) = decode_tag(&mut slice)?;
            match tag {
                1 => msg.tectonics_ticks = decode_u32(&mut slice, wt)?,
                2 => msg.prehistory_ticks = decode_u32(&mut slice, wt)?,
                3 => msg.glaciation_tick = decode_u32(&mut slice, wt)?,
                4 => {
                    let sub = decode_len_slice(&mut slice, wt)?;
                    msg.tiles.push(PrehistoryTile::decode(sub)?);
                }
                5 => {
                    let sub = decode_len_slice(&mut slice, wt)?;
                    msg.groups.push(LithosphericGroup::decode(sub)?);
                }
                6 => {
                    let sub = decode_len_slice(&mut slice, wt)?;
                    msg.heritages.push(Heritage::decode(sub)?);
                }
                _ => skip_field(&mut slice, wt)?,
            }
        }
        Ok(msg)
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct RaceContribution {
    pub heritage_id: u32,
    pub fraction: f32,
}

impl RaceContribution {
    pub fn encode(&self, buf: &mut Vec<u8>) {
        encode_u32_field(1, self.heritage_id, buf);
        encode_f32_field(2, self.fraction, buf);
    }

    pub fn decode(mut slice: &[u8]) -> Result<Self, DecodeError> {
        let mut msg = Self::default();
        while !slice.is_empty() {
            let (tag, wt) = decode_tag(&mut slice)?;
            match tag {
                1 => msg.heritage_id = decode_u32(&mut slice, wt)?,
                2 => msg.fraction = decode_f32(&mut slice, wt)?,
                _ => skip_field(&mut slice, wt)?,
            }
        }
        Ok(msg)
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Community {
    pub thousands: f32,
    pub nationality_id: u32,
    pub language_id: u32,
    pub agriculture: bool,
    pub racial_mix: Vec<RaceContribution>,
}

impl Community {
    pub fn encode(&self, buf: &mut Vec<u8>) {
        encode_f32_field(1, self.thousands, buf);
        encode_u32_field(2, self.nationality_id, buf);
        encode_u32_field(3, self.language_id, buf);
        encode_bool_field(4, self.agriculture, buf);
        for rm in &self.racial_mix {
            encode_submessage(5, buf, |b| rm.encode(b));
        }
    }

    pub fn decode(mut slice: &[u8]) -> Result<Self, DecodeError> {
        let mut msg = Self::default();
        while !slice.is_empty() {
            let (tag, wt) = decode_tag(&mut slice)?;
            match tag {
                1 => msg.thousands = decode_f32(&mut slice, wt)?,
                2 => msg.nationality_id = decode_u32(&mut slice, wt)?,
                3 => msg.language_id = decode_u32(&mut slice, wt)?,
                4 => msg.agriculture = decode_bool(&mut slice, wt)?,
                5 => {
                    let sub = decode_len_slice(&mut slice, wt)?;
                    msg.racial_mix.push(RaceContribution::decode(sub)?);
                }
                _ => skip_field(&mut slice, wt)?,
            }
        }
        Ok(msg)
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PopulatedFineFace {
    pub fine_face_id: u32,
    pub state_id: u32,
    pub communities: Vec<Community>,
}

impl PopulatedFineFace {
    pub fn encode(&self, buf: &mut Vec<u8>) {
        encode_u32_field(1, self.fine_face_id, buf);
        encode_u32_field(2, self.state_id, buf);
        for c in &self.communities {
            encode_submessage(3, buf, |b| c.encode(b));
        }
    }

    pub fn decode(mut slice: &[u8]) -> Result<Self, DecodeError> {
        let mut msg = Self::default();
        while !slice.is_empty() {
            let (tag, wt) = decode_tag(&mut slice)?;
            match tag {
                1 => msg.fine_face_id = decode_u32(&mut slice, wt)?,
                2 => msg.state_id = decode_u32(&mut slice, wt)?,
                3 => {
                    let sub = decode_len_slice(&mut slice, wt)?;
                    msg.communities.push(Community::decode(sub)?);
                }
                _ => skip_field(&mut slice, wt)?,
            }
        }
        Ok(msg)
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
#[repr(u32)]
pub enum ConceptKind {
    #[default]
    Species = 0,
    Nation = 1,
    State = 2,
    Language = 3,
}

impl ConceptKind {
    pub fn from_u32(v: u32) -> Self {
        match v {
            0 => Self::Species,
            1 => Self::Nation,
            2 => Self::State,
            3 => Self::Language,
            _ => Self::Species,
        }
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct Concept {
    pub kind: ConceptKind,
    pub id: u32,
}

impl Concept {
    pub fn encode(&self, buf: &mut Vec<u8>) {
        encode_u32_field(1, self.kind as u32, buf);
        encode_u32_field(2, self.id, buf);
    }

    pub fn decode(mut slice: &[u8]) -> Result<Self, DecodeError> {
        let mut msg = Self::default();
        while !slice.is_empty() {
            let (tag, wt) = decode_tag(&mut slice)?;
            match tag {
                1 => msg.kind = ConceptKind::from_u32(decode_u32(&mut slice, wt)?),
                2 => msg.id = decode_u32(&mut slice, wt)?,
                _ => skip_field(&mut slice, wt)?,
            }
        }
        Ok(msg)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TickSoundChanges {
    pub sound_change_kinds: Vec<u32>,
}

impl TickSoundChanges {
    pub fn encode(&self, buf: &mut Vec<u8>) {
        encode_packed_u32_field(1, &self.sound_change_kinds, buf);
    }

    pub fn decode(mut slice: &[u8]) -> Result<Self, DecodeError> {
        let mut msg = Self::default();
        while !slice.is_empty() {
            let (tag, wt) = decode_tag(&mut slice)?;
            match tag {
                1 => decode_packed_or_single_u32(&mut slice, wt, &mut msg.sound_change_kinds)?,
                _ => skip_field(&mut slice, wt)?,
            }
        }
        Ok(msg)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NeologismBatch {
    pub tick: u32,
    pub rng_state_16b: Vec<u8>,
    pub concept_indices: Vec<u32>,
}

impl NeologismBatch {
    pub fn encode(&self, buf: &mut Vec<u8>) {
        encode_u32_field(1, self.tick, buf);
        encode_bytes_field(2, &self.rng_state_16b, buf);
        encode_packed_u32_field(3, &self.concept_indices, buf);
    }

    pub fn decode(mut slice: &[u8]) -> Result<Self, DecodeError> {
        let mut msg = Self::default();
        while !slice.is_empty() {
            let (tag, wt) = decode_tag(&mut slice)?;
            match tag {
                1 => msg.tick = decode_u32(&mut slice, wt)?,
                2 => msg.rng_state_16b = decode_len_slice(&mut slice, wt)?.to_vec(),
                3 => decode_packed_or_single_u32(&mut slice, wt, &mut msg.concept_indices)?,
                _ => skip_field(&mut slice, wt)?,
            }
        }
        Ok(msg)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LoanBatch {
    pub tick: u32,
    pub source_language_id: u32,
    pub concept_indices: Vec<u32>,
}

impl LoanBatch {
    pub fn encode(&self, buf: &mut Vec<u8>) {
        encode_u32_field(1, self.tick, buf);
        encode_u32_field(2, self.source_language_id, buf);
        encode_packed_u32_field(3, &self.concept_indices, buf);
    }

    pub fn decode(mut slice: &[u8]) -> Result<Self, DecodeError> {
        let mut msg = Self::default();
        while !slice.is_empty() {
            let (tag, wt) = decode_tag(&mut slice)?;
            match tag {
                1 => msg.tick = decode_u32(&mut slice, wt)?,
                2 => msg.source_language_id = decode_u32(&mut slice, wt)?,
                3 => decode_packed_or_single_u32(&mut slice, wt, &mut msg.concept_indices)?,
                _ => skip_field(&mut slice, wt)?,
            }
        }
        Ok(msg)
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct DerivationEntry {
    pub tick: u32,
    pub source_concept: Option<Concept>,
    pub dest_concept_index: u32,
}

impl DerivationEntry {
    pub fn encode(&self, buf: &mut Vec<u8>) {
        encode_u32_field(1, self.tick, buf);
        if let Some(ref c) = self.source_concept {
            encode_submessage(2, buf, |b| c.encode(b));
        }
        encode_u32_field(3, self.dest_concept_index, buf);
    }

    pub fn decode(mut slice: &[u8]) -> Result<Self, DecodeError> {
        let mut msg = Self::default();
        while !slice.is_empty() {
            let (tag, wt) = decode_tag(&mut slice)?;
            match tag {
                1 => msg.tick = decode_u32(&mut slice, wt)?,
                2 => {
                    let sub = decode_len_slice(&mut slice, wt)?;
                    msg.source_concept = Some(Concept::decode(sub)?);
                }
                3 => msg.dest_concept_index = decode_u32(&mut slice, wt)?,
                _ => skip_field(&mut slice, wt)?,
            }
        }
        Ok(msg)
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct RedefinitionEntry {
    pub tick: u32,
    pub source_concept_index: u32,
    pub dest_concept_index: u32,
}

impl RedefinitionEntry {
    pub fn encode(&self, buf: &mut Vec<u8>) {
        encode_u32_field(1, self.tick, buf);
        encode_u32_field(2, self.source_concept_index, buf);
        encode_u32_field(3, self.dest_concept_index, buf);
    }

    pub fn decode(mut slice: &[u8]) -> Result<Self, DecodeError> {
        let mut msg = Self::default();
        while !slice.is_empty() {
            let (tag, wt) = decode_tag(&mut slice)?;
            match tag {
                1 => msg.tick = decode_u32(&mut slice, wt)?,
                2 => msg.source_concept_index = decode_u32(&mut slice, wt)?,
                3 => msg.dest_concept_index = decode_u32(&mut slice, wt)?,
                _ => skip_field(&mut slice, wt)?,
            }
        }
        Ok(msg)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LanguageHistory {
    pub concepts: Vec<Concept>,
    pub changes_per_tick: Vec<TickSoundChanges>,
    pub neologisms: Vec<NeologismBatch>,
    pub loans: Vec<LoanBatch>,
    pub derivations: Vec<DerivationEntry>,
    pub redefinitions: Vec<RedefinitionEntry>,
}

impl LanguageHistory {
    pub fn encode(&self, buf: &mut Vec<u8>) {
        for c in &self.concepts {
            encode_submessage(1, buf, |b| c.encode(b));
        }
        for cpt in &self.changes_per_tick {
            encode_submessage(2, buf, |b| cpt.encode(b));
        }
        for n in &self.neologisms {
            encode_submessage(3, buf, |b| n.encode(b));
        }
        for l in &self.loans {
            encode_submessage(4, buf, |b| l.encode(b));
        }
        for d in &self.derivations {
            encode_submessage(5, buf, |b| d.encode(b));
        }
        for r in &self.redefinitions {
            encode_submessage(6, buf, |b| r.encode(b));
        }
    }

    pub fn decode(mut slice: &[u8]) -> Result<Self, DecodeError> {
        let mut msg = Self::default();
        while !slice.is_empty() {
            let (tag, wt) = decode_tag(&mut slice)?;
            match tag {
                1 => {
                    let sub = decode_len_slice(&mut slice, wt)?;
                    msg.concepts.push(Concept::decode(sub)?);
                }
                2 => {
                    let sub = decode_len_slice(&mut slice, wt)?;
                    msg.changes_per_tick.push(TickSoundChanges::decode(sub)?);
                }
                3 => {
                    let sub = decode_len_slice(&mut slice, wt)?;
                    msg.neologisms.push(NeologismBatch::decode(sub)?);
                }
                4 => {
                    let sub = decode_len_slice(&mut slice, wt)?;
                    msg.loans.push(LoanBatch::decode(sub)?);
                }
                5 => {
                    let sub = decode_len_slice(&mut slice, wt)?;
                    msg.derivations.push(DerivationEntry::decode(sub)?);
                }
                6 => {
                    let sub = decode_len_slice(&mut slice, wt)?;
                    msg.redefinitions.push(RedefinitionEntry::decode(sub)?);
                }
                _ => skip_field(&mut slice, wt)?,
            }
        }
        Ok(msg)
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct StatePair {
    pub state_a: u32,
    pub state_b: u32,
}

impl StatePair {
    pub fn encode(&self, buf: &mut Vec<u8>) {
        encode_u32_field(1, self.state_a, buf);
        encode_u32_field(2, self.state_b, buf);
    }

    pub fn decode(mut slice: &[u8]) -> Result<Self, DecodeError> {
        let mut msg = Self::default();
        while !slice.is_empty() {
            let (tag, wt) = decode_tag(&mut slice)?;
            match tag {
                1 => msg.state_a = decode_u32(&mut slice, wt)?,
                2 => msg.state_b = decode_u32(&mut slice, wt)?,
                _ => skip_field(&mut slice, wt)?,
            }
        }
        Ok(msg)
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct HistoryStage {
    pub prehistory_base: Option<PrehistoryStage>,
    pub history_ticks: u32,
    pub populated_faces: Vec<PopulatedFineFace>,
    pub languages: Vec<LanguageHistory>,
    pub trade_partners: Vec<StatePair>,
    pub conflicts: Vec<StatePair>,
}

impl HistoryStage {
    pub fn encode(&self, buf: &mut Vec<u8>) {
        if let Some(ref pb) = self.prehistory_base {
            encode_submessage(1, buf, |b| pb.encode(b));
        }
        encode_u32_field(2, self.history_ticks, buf);
        for pf in &self.populated_faces {
            encode_submessage(3, buf, |b| pf.encode(b));
        }
        for l in &self.languages {
            encode_submessage(4, buf, |b| l.encode(b));
        }
        for tp in &self.trade_partners {
            encode_submessage(5, buf, |b| tp.encode(b));
        }
        for c in &self.conflicts {
            encode_submessage(6, buf, |b| c.encode(b));
        }
    }

    pub fn decode(mut slice: &[u8]) -> Result<Self, DecodeError> {
        let mut msg = Self::default();
        while !slice.is_empty() {
            let (tag, wt) = decode_tag(&mut slice)?;
            match tag {
                1 => {
                    let sub = decode_len_slice(&mut slice, wt)?;
                    msg.prehistory_base = Some(PrehistoryStage::decode(sub)?);
                }
                2 => msg.history_ticks = decode_u32(&mut slice, wt)?,
                3 => {
                    let sub = decode_len_slice(&mut slice, wt)?;
                    msg.populated_faces.push(PopulatedFineFace::decode(sub)?);
                }
                4 => {
                    let sub = decode_len_slice(&mut slice, wt)?;
                    msg.languages.push(LanguageHistory::decode(sub)?);
                }
                5 => {
                    let sub = decode_len_slice(&mut slice, wt)?;
                    msg.trade_partners.push(StatePair::decode(sub)?);
                }
                6 => {
                    let sub = decode_len_slice(&mut slice, wt)?;
                    msg.conflicts.push(StatePair::decode(sub)?);
                }
                _ => skip_field(&mut slice, wt)?,
            }
        }
        Ok(msg)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum PlanetStageOneof {
    Tectonics(TectonicsStage),
    Prehistory(PrehistoryStage),
    History(HistoryStage),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PlanetSnapshot {
    pub rng_state_16b: Vec<u8>,
    pub grid_size: u32,
    pub radius_km: f32,
    pub spin: f32,
    pub tilt_deg: f32,
    pub stage: Option<PlanetStageOneof>,
}

impl PlanetSnapshot {
    pub fn encode(&self, buf: &mut Vec<u8>) {
        encode_bytes_field(1, &self.rng_state_16b, buf);
        encode_u32_field(2, self.grid_size, buf);
        encode_f32_field(3, self.radius_km, buf);
        encode_f32_field(4, self.spin, buf);
        encode_f32_field(5, self.tilt_deg, buf);
        if let Some(ref st) = self.stage {
            match st {
                PlanetStageOneof::Tectonics(tec) => encode_submessage(6, buf, |b| tec.encode(b)),
                PlanetStageOneof::Prehistory(pre) => encode_submessage(7, buf, |b| pre.encode(b)),
                PlanetStageOneof::History(his) => encode_submessage(8, buf, |b| his.encode(b)),
            }
        }
    }

    pub fn encode_to_vec(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        self.encode(&mut buf);
        buf
    }

    pub fn decode(mut slice: &[u8]) -> Result<Self, DecodeError> {
        let mut msg = Self::default();
        while !slice.is_empty() {
            let (tag, wt) = decode_tag(&mut slice)?;
            match tag {
                1 => {
                    let sub = decode_len_slice(&mut slice, wt)?;
                    msg.rng_state_16b = sub.to_vec();
                }
                2 => msg.grid_size = decode_u32(&mut slice, wt)?,
                3 => msg.radius_km = decode_f32(&mut slice, wt)?,
                4 => msg.spin = decode_f32(&mut slice, wt)?,
                5 => msg.tilt_deg = decode_f32(&mut slice, wt)?,
                6 => {
                    let sub = decode_len_slice(&mut slice, wt)?;
                    msg.stage = Some(PlanetStageOneof::Tectonics(TectonicsStage::decode(sub)?));
                }
                7 => {
                    let sub = decode_len_slice(&mut slice, wt)?;
                    msg.stage = Some(PlanetStageOneof::Prehistory(PrehistoryStage::decode(sub)?));
                }
                8 => {
                    let sub = decode_len_slice(&mut slice, wt)?;
                    msg.stage = Some(PlanetStageOneof::History(HistoryStage::decode(sub)?));
                }
                _ => skip_field(&mut slice, wt)?,
            }
        }
        Ok(msg)
    }

    /// Extract the geological tiles from whichever simulation stage is stored in the snapshot.
    pub fn geological_tiles(&self) -> Option<Vec<&GeologicalTile>> {
        match self.stage.as_ref()? {
            PlanetStageOneof::Tectonics(t) => Some(t.tiles.iter().collect()),
            PlanetStageOneof::Prehistory(p) => {
                p.tiles.iter().map(|pt| pt.geology.as_ref()).collect()
            }
            PlanetStageOneof::History(h) => h
                .prehistory_base
                .as_ref()?
                .tiles
                .iter()
                .map(|pt| pt.geology.as_ref())
                .collect(),
        }
    }

    /// Return the tectonic ticks (`geoticks`) recorded in the snapshot.
    pub fn tectonics_ticks(&self) -> u32 {
        match self.stage.as_ref() {
            Some(PlanetStageOneof::Tectonics(t)) => t.tectonics_ticks,
            Some(PlanetStageOneof::Prehistory(p)) => p.tectonics_ticks,
            Some(PlanetStageOneof::History(h)) => h
                .prehistory_base
                .as_ref()
                .map(|p| p.tectonics_ticks)
                .unwrap_or(0),
            None => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_planet_snapshot_proto3_wire_roundtrip() {
        let snap = PlanetSnapshot {
            rng_state_16b: vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16],
            grid_size: 5,
            radius_km: 3700.0,
            spin: 1.0,
            tilt_deg: 23.0,
            stage: Some(PlanetStageOneof::Tectonics(TectonicsStage {
                tectonics_ticks: 42,
                atmosphere_ticks_remaining: 0,
                life_ticks_remaining: 0,
                people_ticks_remaining: 0,
                tiles: vec![
                    GeologicalTile {
                        bottom: -10.5,
                        mountainosity: 0.0,
                        layers: vec![RockLayer {
                            rock: Some(Rock {
                                kind: Some(RockKindOneof::Igneous(IgneousRock {
                                    is_intrusive: false,
                                    felsity: 0.5,
                                })),
                            }),
                            thickness: 5.0,
                        }],
                    },
                    GeologicalTile {
                        bottom: -34.0,
                        mountainosity: 6.5,
                        layers: vec![
                            RockLayer {
                                rock: Some(Rock {
                                    kind: Some(RockKindOneof::Metamorphic(MetamorphicRock {
                                        facies: MetamorphicFacies::Greenschist,
                                        toughness: 0.75,
                                        felsity: Some(0.68),
                                        calcity: None,
                                        bogginess: None,
                                    })),
                                }),
                                thickness: 25.0,
                            },
                            RockLayer {
                                rock: Some(Rock {
                                    kind: Some(RockKindOneof::Sedimentary(SedimentaryRock {
                                        clasticity: 2.0,
                                        felsity: Some(0.72),
                                        calcity: Some(0.82),
                                        bogginess: None,
                                    })),
                                }),
                                thickness: 11.2,
                            },
                        ],
                    },
                ],
                groups: vec![LithosphericGroup {
                    tile_ids: vec![1, 5, 19, 300],
                    velocity: Some(Vec3 {
                        x: 0.01,
                        y: -0.02,
                        z: 0.005,
                    }),
                }],
                ridges: vec![SpreadingRidgeArc {
                    start: Some(Vec3 {
                        x: 1.0,
                        y: 0.0,
                        z: 0.0,
                    }),
                    end: Some(Vec3 {
                        x: 0.0,
                        y: 1.0,
                        z: 0.0,
                    }),
                    velocity: Some(Vec3 {
                        x: 0.0,
                        y: 0.0,
                        z: 0.0,
                    }),
                    age_ticks: 4,
                }],
            })),
        };

        let bytes = snap.encode_to_vec();
        // Field 1 (`rng_state_16b`, tag = (1 << 3) | 2 = 0x0A, length = 16 = 0x10)
        assert_eq!(bytes[0], 0x0A);
        assert_eq!(bytes[1], 0x10);

        let decoded = PlanetSnapshot::decode(&bytes).expect("valid proto3 wire bytes");
        assert_eq!(decoded, snap);
    }
}

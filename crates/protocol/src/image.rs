const OPCODE: u8 = 0x50;

pub fn build_image_chunk(index: u32, payload: &[u8; 1020]) -> [u8; 1024] {
    let mut buf = [0u8; 1024];
    let idx_bytes = index.to_le_bytes();
    buf[0] = idx_bytes[0];
    buf[1] = idx_bytes[1];
    buf[2] = idx_bytes[2];
    buf[3] = OPCODE;
    buf[4..1024].copy_from_slice(payload);
    buf
}

pub fn build_image_final(total_size: u32, x: u16, y: u16) -> [u8; 1024] {
    let mut buf = [0u8; 1024];
    // Terminator marker
    buf[0] = 0xFF;
    buf[1] = 0xFF;
    buf[2] = 0xFF;
    buf[3] = 0xFF;
    // Total payload size (LE)
    let size_bytes = total_size.to_le_bytes();
    buf[4] = size_bytes[0];
    buf[5] = size_bytes[1];
    buf[6] = size_bytes[2];
    buf[7] = size_bytes[3];
    // X position (LE)
    let x_bytes = x.to_le_bytes();
    buf[8] = x_bytes[0];
    buf[9] = x_bytes[1];
    // Y position (LE)
    let y_bytes = y.to_le_bytes();
    buf[10] = y_bytes[0];
    buf[11] = y_bytes[1];
    buf
}

pub struct ImageChunker<'a> {
    data: &'a [u8],
    x: u16,
    y: u16,
    offset: usize,
    index: u32,
    done: bool,
}

impl<'a> ImageChunker<'a> {
    pub fn new(data: &'a [u8], x: u16, y: u16) -> Self {
        Self {
            data,
            x,
            y,
            offset: 0,
            index: 0,
            done: false,
        }
    }
}

impl<'a> Iterator for ImageChunker<'a> {
    type Item = [u8; 1024];

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }

        if self.offset >= self.data.len() {
            // Emit final terminator packet
            self.done = true;
            return Some(build_image_final(self.data.len() as u32, self.x, self.y));
        }

        let remaining = self.data.len() - self.offset;
        let mut payload = [0u8; 1020];
        let copy_len = remaining.min(1020);
        payload[..copy_len].copy_from_slice(&self.data[self.offset..self.offset + copy_len]);

        let chunk = build_image_chunk(self.index, &payload);
        self.offset += copy_len;
        self.index += 1;
        Some(chunk)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_header_format() {
        let payload = [0xAA; 1020];
        let chunk = build_image_chunk(0, &payload);
        assert_eq!(chunk[0], 0x00); // index byte 0
        assert_eq!(chunk[1], 0x00); // index byte 1
        assert_eq!(chunk[2], 0x00); // index byte 2
        assert_eq!(chunk[3], OPCODE);
        assert_eq!(chunk[4], 0xAA); // first payload byte
        assert_eq!(chunk[1023], 0xAA); // last payload byte
    }

    #[test]
    fn chunk_index_little_endian() {
        let payload = [0u8; 1020];
        let chunk = build_image_chunk(0x020100, &payload);
        assert_eq!(chunk[0], 0x00);
        assert_eq!(chunk[1], 0x01);
        assert_eq!(chunk[2], 0x02);
        assert_eq!(chunk[3], OPCODE);
    }

    #[test]
    fn final_packet_format() {
        let final_pkt = build_image_final(12345, 100, 200);
        assert_eq!(final_pkt[0], 0xFF);
        assert_eq!(final_pkt[1], 0xFF);
        assert_eq!(final_pkt[2], 0xFF);
        assert_eq!(final_pkt[3], 0xFF);
        // total_size = 12345 = 0x3039 LE
        assert_eq!(u32::from_le_bytes([final_pkt[4], final_pkt[5], final_pkt[6], final_pkt[7]]), 12345);
        // x = 100 LE
        assert_eq!(u16::from_le_bytes([final_pkt[8], final_pkt[9]]), 100);
        // y = 200 LE
        assert_eq!(u16::from_le_bytes([final_pkt[10], final_pkt[11]]), 200);
    }

    #[test]
    fn chunker_empty_data() {
        let data = [];
        let chunks: Vec<_> = ImageChunker::new(&data, 0, 0).collect();
        assert_eq!(chunks.len(), 1); // just the final packet
        assert_eq!(chunks[0][0..4], [0xFF, 0xFF, 0xFF, 0xFF]);
    }

    #[test]
    fn chunker_small_data() {
        let data = [0xBB; 500];
        let chunks: Vec<_> = ImageChunker::new(&data, 10, 20).collect();
        assert_eq!(chunks.len(), 2); // 1 data chunk + 1 final
        // First chunk: data
        assert_eq!(chunks[0][3], OPCODE);
        assert_eq!(chunks[0][4], 0xBB);
        // Second chunk: final
        assert_eq!(chunks[1][0..4], [0xFF, 0xFF, 0xFF, 0xFF]);
    }

    #[test]
    fn chunker_exact_boundary() {
        let data = [0xCC; 1020];
        let chunks: Vec<_> = ImageChunker::new(&data, 0, 0).collect();
        assert_eq!(chunks.len(), 2); // 1 full data chunk + final
    }

    #[test]
    fn chunker_multi_chunk() {
        let data = [0xDD; 2050];
        let chunks: Vec<_> = ImageChunker::new(&data, 0, 0).collect();
        // 2050 bytes / 1020 per chunk = 3 data chunks (1020 + 1020 + 10) + 1 final
        assert_eq!(chunks.len(), 4);
        // Check indices increment
        assert_eq!(chunks[0][0], 0); // index 0
        assert_eq!(chunks[1][0], 1); // index 1
        assert_eq!(chunks[2][0], 2); // index 2
        assert_eq!(chunks[3][0..4], [0xFF, 0xFF, 0xFF, 0xFF]); // final
    }
}

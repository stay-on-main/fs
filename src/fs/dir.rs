
use super::stream::Stream;
use crate::fs::stream::{Seek, Read, Write};

struct DirIterator<'stream, 'bd: 'stream> {
    stream: Stream<'bd, 'stream>,
}

const ATTR_LONG_FILE_NAME: u8 = 0x0f;

pub struct DirEntry {
    //void fat_get_file_modification_date(const struct fat_dir_entry_struct* dir_entry, uint16_t* year, uint8_t* month, uint8_t* day);
//void fat_get_file_modification_time(const struct fat_dir_entry_struct* dir_entry, uint8_t* hour, uint8_t* min, uint8_t* sec);
    data: [u8; 32],
}

impl DirEntry {
    pub fn new(data: [u8; 32]) -> Self {
        for &d in data[..11].iter() {
            /*
            if (d >= b'0' && d <= b'9') || (d >= b'a' && d <= b'z') || (d >= b'A' && d <= b'Z') {
                print!(" {} ", d as char);
            } else {
                print!("__ ");
            }
            */
            print!("{}", d as char);
        }
        println!();
        Self { data }
    }
}

impl <'stream, 'bd: 'stream> Iterator for DirIterator<'stream, 'bd> {
    type Item = DirEntry;

    fn next(&mut self) -> Option<Self::Item> {
        let mut data = [0u8; 32];

        loop {
            match self.stream.read(&mut data[..32]) {
                Ok(count) => {
                    if count != data.len() {
                        // we are at the end of folder
                        return None;
                    }

                    if data[0] == 0 {
                        // no more dir entries in folder
                        return None;
                    }

                    if data[0] == 0xe5 {
                        // this is deleted entry
                        continue;
                    }

                    if  (data[11] & ATTR_LONG_FILE_NAME) == ATTR_LONG_FILE_NAME {
                        continue;
                    }

                    return Some(DirEntry::new(data));
                    /*
                    if entry.compare(name) {
                        return Ok(entry);
                    }
                    */
                },
                Err(e) => return None,
            }
        }
    }
}
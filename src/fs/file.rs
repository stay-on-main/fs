
use super::stream::{Stream, SeekFrom};
use super::sector::FsErr;
use crate::fs::stream::{Seek, Read, Write};

struct File<'stream, 'bd: 'stream> {
    stream: Stream<'bd, 'stream>,
    size: u32,
}

impl <'stream, 'bd: 'stream> File <'stream, 'bd> {
    pub fn read(&mut self, buff: &mut[u8]) -> Result<usize, FsErr> {
        let pos = self.stream.seek(SeekFrom::Current(0))?;

        if pos == self.size {
            return Err(FsErr::EndOfFile);
        }

        let bytes_to_read = core::cmp::min(buff.len(), (self.size - pos) as usize);
        let mut bytes_read = 0;

        while bytes_read < bytes_to_read {
            match self.stream.read(&mut buff[bytes_read..bytes_to_read]) {
                Ok(len) => bytes_read += len,
                Err(e) => {
                    if bytes_read == 0 {
                        return Err(e);
                    }

                    break;
                }
            }
        }
        
        Ok(bytes_read)
    }

    pub fn write(&mut self, buff: &[u8]) -> Result<usize, FsErr> {
        let mut bytes_written = 0;

        while bytes_written != buff.len() {
            match self.stream.write(&buff[bytes_written..]) {
                Ok(len) => bytes_written += len,
                Err(e) => {
                    if bytes_written == 0 {
                        return Err(e);
                    }

                    break;
                }
            }
        }

        Ok(bytes_written)
    }

    pub fn seek(&mut self, offset: SeekFrom) -> Result<(), FsErr> {
        self.stream.seek(offset)?;
        // need to check file border
        Ok(())
    }
    
    pub fn resize(&mut self, size: u32) -> Result<(), FsErr> {
        todo!();
    }
    
    pub fn close(mut self) -> Result<(), FsErr> {
        self.stream.flush()
    }
}
use super::fs::{Fs, ClusterValue};
use super::sector::FsErr;

pub enum SeekFrom {
    Start(u32),
    Current(i32),
    End(i32),
}

pub trait Read {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, FsErr>;
}

pub trait Write {
    fn write(&mut self, buf: &[u8]) -> Result<usize, FsErr>;
    fn flush(&mut self) -> Result<(), FsErr>;
}

pub trait Seek {
    fn seek(&mut self, pos: SeekFrom) -> Result<u32, FsErr>;
}

pub struct Stream<'stream, 'bd: 'stream> {
    fs: &'stream Fs<'bd>,
    first_cluster: u32,

    cluster: u32,
    sector: u32,
    offset: usize,
    global_offset: u32,
}

impl <'stream, 'bd: 'stream> Stream<'stream, 'bd> {
    fn go_to_next_sector_if_necessary(&mut self) -> Result<(), FsErr> {
        if (self.offset as u32) < self.fs.sector_size {
            return Ok(());
        }

        if self.sector + 1 < self.fs.sectors_in_cluster {
            self.sector += 1;
            self.offset = 0;
            return Ok(());
        }

        match self.fs.table_get(self.cluster)? {
            ClusterValue::Next(next) => {
                self.cluster = next;
                self.sector = 0;
                self.offset = 0;
                return Ok(());
            }

            ClusterValue::Last => return Err(FsErr::EndOfStream),
            ClusterValue::Bad | ClusterValue::Free => return Err(FsErr::FatTableError),
        }
    }

    pub fn set_len(&mut self, cluster_count: u32) -> Result<(), FsErr> {
        self.fs.table_chain_set_len(self.first_cluster, cluster_count)
    }
}

impl <'stream, 'bd> Read for Stream<'stream, 'bd> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, FsErr> {
        self.go_to_next_sector_if_necessary()?;

        let len = core::cmp::min(buf.len(), (self.fs.sector_size as usize) - self.offset);
        let sector = self.fs.cluster_to_sector(self.cluster) + self.sector;
        self.fs.sector.read(sector, self.offset, &mut buf[..len])?;
        self.offset += len;
        self.global_offset += len as u32;
        Ok(len)
    }
}

impl <'stream, 'bd> Write for Stream<'stream, 'bd> {
    fn write(&mut self, buf: &[u8]) -> Result<usize, FsErr> {
        match self.go_to_next_sector_if_necessary() {
            Err(FsErr::EndOfStream) => {
                let next = self.fs.table_chain_extend(self.cluster, 1)?;
                self.cluster = next;
                self.sector = 0;
                self.offset = 0;
                return Ok(0);
            },
            Err(e) => return Err(e),
            _ => {},
        }
        let len = core::cmp::min(buf.len(), (self.fs.sector_size as usize) - self.offset);
        let sector = self.fs.cluster_to_sector(self.cluster) + self.sector;
        self.fs.sector.write(sector, self.offset, &buf[..len])?;
        self.offset += len;
        self.global_offset += len as u32;
        Ok(len)
    }

    fn flush(&mut self) -> Result<(), FsErr> {
        self.fs.sector.flush()
    }
}

impl <'stream, 'bd> Seek for Stream<'stream, 'bd> {
    fn seek(&mut self, pos: SeekFrom) -> Result<u32, FsErr> {
        let new_pos = match pos {
            SeekFrom::Start(start) => start,
            SeekFrom::Current(current) => {
                if (self.global_offset as i64) + (current as i64) < 0 {
                    return Err(FsErr::NegativeSeek);
                }

                ((self.global_offset as i32) + current) as u32
            },
            SeekFrom::End(end) => todo!(),
        };

        self.cluster = self.fs.table_chain_skip(self.first_cluster, new_pos / self.fs.cluster_size)?;
        self.sector = (new_pos % self.fs.cluster_size) / self.fs.sector_size;
        self.offset = (new_pos % self.fs.sector_size) as usize;
        self.global_offset = new_pos;
        Ok(self.global_offset)
    }
}
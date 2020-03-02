use core::cell::RefCell;


#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum FsErr {
    BadBlockSize,
    PartitionOutOfStorageSpace,
    ReadError,
    WriteError,
    OutOfRange,
    DirEntryNotFile,
    FileOrFolderDesntExist,
    FatTableError,
    NoFreeCluster,
    BadCount,
    NegativeSeek,
    EndOfFile,
    UnExpectedEndOfFile,
    EndOfStream,
}

const BLOCK_MAX_SIZE: usize = 4096;
const BLOCK_MIN_SIZE: usize = 512;

pub trait BlockDeviceIo {
    fn block_size(&self) -> u32;
    fn block_count(&self) -> u32;
    fn read(&self, block: u32, data: &mut [u8]) -> Result<(), FsErr>;
    fn write(&self, block: u32, data: &[u8]) -> Result<(), FsErr>;
}

pub struct BlockDeviceCache<'bd> {
    io: &'bd dyn BlockDeviceIo,
    block_size: usize,
    block_count: u32,

    cached_block: u32,
    data: [u8; BLOCK_MAX_SIZE],
    dirty: bool,
}

pub trait BlockDevice {
    fn block_size(&self) -> u32;
    fn get(&mut self, number: u32) -> Result<&[u8], FsErr>;
    fn get_mut(&mut self, number: u32) -> Result<&mut [u8], FsErr>;
    fn flush(&mut self) -> Result<(), FsErr>;
}

impl <'bd> BlockDevice for BlockDeviceCache <'bd> {
    fn block_size(&self) -> u32 {
        self.block_size as u32
    }

    fn get(&mut self, number: u32) -> Result<&[u8], FsErr> {
        self.sync(number)?;
        Ok(&self.data[..self.block_size])
    }
    
    fn get_mut(&mut self, number: u32) -> Result<&mut [u8], FsErr> {
        self.sync(number)?;
        self.dirty = true;
        Ok(&mut self.data[..self.block_size])
    }

    fn flush(&mut self) -> Result<(), FsErr> {
        if self.dirty {
            self.io.write(self.cached_block, &self.data[..self.block_size])?;
            self.dirty = false;
        }
        Ok(())
    }
}

impl <'bd> BlockDeviceCache <'bd> {
    pub fn new(io: &'bd dyn BlockDeviceIo) -> Self {
        Self {
            io,
            block_size: io.block_size() as usize,
            block_count: io.block_count(),
            cached_block: core::u32::MAX,
            data: [0u8; BLOCK_MAX_SIZE],
            dirty: false,
        }
    }

    fn sync(&mut self, number: u32) -> Result<(), FsErr> {
        if number != self.cached_block {
            if number >= self.block_count {
                return Err(FsErr::OutOfRange);
            }

            self.flush()?;
            self.io.read(number, &mut self.data[..self.block_size])?;
            self.cached_block = number;
        }
        Ok(())
    }
}

pub struct Sector<'bd> {
    sector: RefCell<BlockDeviceCache<'bd>>,
}

impl <'bd> Sector<'bd> {
    pub fn new(io: &'bd dyn BlockDeviceIo) -> Self {
        Self { sector: RefCell::new(BlockDeviceCache::new(io)) }
    }

    pub fn read(&self, sector: u32, offset: usize, buff: &mut [u8]) -> Result<(), FsErr> {
        let mut s = self.sector.borrow_mut();
        let data = s.get(sector)?;
        let len = buff.len();
        buff[..].copy_from_slice(&data[offset..(offset + len)]);
        Ok(())
    }

    pub fn write(&self, sector: u32, offset: usize, buff: &[u8]) -> Result<(), FsErr> {
        let mut s = self.sector.borrow_mut();
        let data = s.get_mut(sector)?;
        let len = buff.len();
        data[offset..(offset + len)].copy_from_slice(buff);
        Ok(())
    }

    pub fn flush(&self) -> Result<(), FsErr> {
        let mut s = self.sector.borrow_mut();
        s.flush()
    }
}
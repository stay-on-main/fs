use super::sector::{BlockDeviceIo, Sector, FsErr};

pub enum FatType {
    Fat32,
    Fat16,
    Fat12,
}

pub struct Fs<'bd> {
    io: &'bd dyn BlockDeviceIo,
    pub sector: Sector<'bd>,

    fat_type: FatType,
    table_clusters_count: u32,
    table_first_sector: u32,

    pub sector_size: u32,
    pub cluster_size: u32,
    pub sectors_in_cluster: u32,
}

pub enum ClusterValue {
    Next(u32),
    Last,
    Free,
    Bad,
}

fn u32_from_bytes(bytes: &[u8]) -> u32 {
    u32::from(bytes[0]) | (u32::from(bytes[1]) << 8) |
    (u32::from(bytes[2]) << 16) | (u32::from(bytes[3]) << 24)
}

impl <'bd> Fs<'bd> {
    fn fat32_cluster_to_sector_and_offset(&self, cluster: u32) -> (u32, usize) {
        let sector = self.table_first_sector + cluster / (self.sector_size >> 2);
        let offset = ((cluster * 4) % self.sector_size) as usize;
        (sector, offset)
    }

    fn fat16_cluster_to_sector_and_offset(&self, cluster: u32) -> (u32, usize) {
        let sector = self.table_first_sector + (cluster * 2) / self.sector_size;
        let offset = ((cluster * 2) % self.sector_size) as usize;
        (sector, offset)
    }

    fn fat12_cluster_to_sector_and_offset(&self, cluster: u32) -> (u32, usize) {
        let sector = self.table_first_sector + (cluster + (cluster / 2)) / self.sector_size;
        let offset = ((cluster + (cluster / 2)) % self.sector_size) as usize;
        (sector, offset)
    }

    pub fn table_get(&self, cluster: u32) -> Result<ClusterValue, FsErr> {
        match self.fat_type {
            FatType::Fat32 => {
                let (sector, offset) = self.fat32_cluster_to_sector_and_offset(cluster);
                let mut buff = [0u8; 4];
                self.sector.read(sector, offset, &mut buff)?;
                let val = u32_from_bytes(&buff[..]) & 0x0FFF_FFFF;

                match val {
                    0 => Ok(ClusterValue::Free),
                    0x0FFF_FFF7 => Ok(ClusterValue::Bad),
                    0x0FFF_FFF8..=core::u32::MAX => Ok(ClusterValue::Last),
                    value => Ok(ClusterValue::Next(value)),
                }
            },
            FatType::Fat16 => {
                let (sector, offset) = self.fat16_cluster_to_sector_and_offset(cluster);
                let mut buff = [0u8; 2];
                self.sector.read(sector, offset, &mut buff)?;
                let val = (buff[0] as u32) | ((buff[1] as u32) << 8);
        
                match val {
                    0 => Ok(ClusterValue::Free),
                    0xFFF7 => Ok(ClusterValue::Bad),
                    0xFFF8..=0xFFFF => Ok(ClusterValue::Last),
                    value => Ok(ClusterValue::Next(value as u32)),
                }
            },
            FatType::Fat12 => {
                let (sector, offset) = self.fat12_cluster_to_sector_and_offset(cluster);
                let mut buff = [0u8; 2];
                self.sector.read(sector, offset, &mut buff)?;
                let val = (buff[0] as u32) | ((buff[1] as u32) << 8);
        
                let val = if cluster & 1 == 0 {
                    (val & 0x0FFF) as u32
                } else {
                    (val >> 4) as u32
                };
        
                match val {
                    0 => Ok(ClusterValue::Free),
                    0xFF7 => Ok(ClusterValue::Bad),
                    0xFF8..=0xFFF => Ok(ClusterValue::Last),
                    value => Ok(ClusterValue::Next(value)),
                }
            }
        }
    }

    pub fn table_set(&self, cluster: u32, value: ClusterValue) -> Result<(), FsErr> {
        match self.fat_type {
            FatType::Fat32 => {
                let val = match value {
                    ClusterValue::Free => 0,
                    ClusterValue::Bad => 0x0FFF_FFF7,
                    ClusterValue::Last => 0x0FFF_FFF8,
                    ClusterValue::Next(n) => n,
                };
                
                let buff = [val as u8, (val >> 8) as u8, (val >> 16) as u8, (val >> 24) as u8];
                let (sector, offset) = self.fat32_cluster_to_sector_and_offset(cluster);
                self.sector.write(sector, offset, &buff)
            },
            FatType::Fat16 => {
                let value = match value {
                    ClusterValue::Next(n) => n & 0xFFFF,
                    ClusterValue::Last => 0xFFF8,
                    ClusterValue::Free => 0,
                    ClusterValue::Bad => 0xFFF7,
                } as u16;
                
                let buff = [value as u8, (value >> 8) as u8];
                let (sector, offset) = self.fat16_cluster_to_sector_and_offset(cluster);
                self.sector.write(sector, offset, &buff)
            },
            FatType::Fat12 => {
                let value = match value {
                    ClusterValue::Next(n) => n & 0xFFF,
                    ClusterValue::Last => 0xFF8,
                    ClusterValue::Free => 0,
                    ClusterValue::Bad => 0xFF7,
                };

                let (sector, offset) = self.fat12_cluster_to_sector_and_offset(cluster);
                let mut buff = [0u8; 2];
                self.sector.read(sector, offset, &mut buff)?;

                if cluster & 1 == 0 {
                    buff[0] = value as u8;
                    buff[1] = (buff[1] & 0x0f) | (((value >> 8) & 0x0f) as u8);
                } else {
                    buff[0] = (buff[0] & 0xf0) | ((value << 4) as u8);
                    buff[1] = (value >> 8) as u8;
                }

                self.sector.write(sector, offset, &buff)
            }
        }
    }

    fn table_find_free(&self, start_cluster: u32) -> Result<u32, FsErr> {
        for cluster in start_cluster..self.table_clusters_count {
            match self.table_get(cluster)? {
                ClusterValue::Free => return Ok(cluster),
                _ => {},
            }
        }

        Err(FsErr::NoFreeCluster)
    }

    pub fn table_chain_skip(&self, cluster: u32, count: u32) -> Result<u32, FsErr> {
        let mut cluster = cluster;
    
        for _ in 0..count {
            match self.table_get(cluster)? {
                ClusterValue::Next(c) => cluster = c,
                ClusterValue::Bad | ClusterValue::Free | ClusterValue::Last => return Err(FsErr::FatTableError),
            }
        }
    
        Ok(cluster)
    }

    pub fn table_chain_delete(&self, cluster: u32) -> Result<(), FsErr> {
        let mut cluster = cluster;
    
        loop {
            let value = self.table_get(cluster)?;
            self.table_set(cluster, ClusterValue::Free)?;
    
            match value {
                ClusterValue::Next(n) => cluster = n,
                ClusterValue::Last => return Ok(()),
                ClusterValue::Free | ClusterValue::Bad => return Err(FsErr::FatTableError),
            }
        }
    }

    pub fn table_chain_set_len(&self, cluster: u32, count: u32) -> Result<(), FsErr> {
        todo!();
    }
    /*
    pub fn table_chain_truncate(&self, cluster: u32, count: u32) -> Result<(), FsErr> {
        assert_ne!(count, 0);

        let count = count - 1;
        let mut cluster = cluster;
        // skip clusters
        for _ in 0..count {
            match self.table_get(cluster)? {
                ClusterValue::Next(n) => cluster = n,
                // chain is shortter
                ClusterValue::Last => return Ok(()),
                ClusterValue::Free | ClusterValue::Bad => return Err(FsErr::FatTableError),
            }
        }
        // truncate chain
        match self.table_get(cluster)? {
            ClusterValue::Next(n) => {
                self.table_set(cluster, ClusterValue::Last)?;
                cluster = n
            },
            // chain is shortter
            ClusterValue::Last => return Ok(()),
            ClusterValue::Free | ClusterValue::Bad => return Err(FsErr::FatTableError),
        }
        // free chain
        self.table_chain_delete(cluster)
    }
    */
    pub fn table_chain_create(&self, count: u32) -> Result<u32, FsErr> {
        assert_ne!(count, 0);
    
        let first_cluster = self.table_find_free(0)?;
        let mut cluster = first_cluster;
        let count = count - 1;
    
        for _ in 0..count {
            let next_cluster = self.table_find_free(cluster + 1)?;
            self.table_set(cluster, ClusterValue::Next(next_cluster))?;
            cluster = next_cluster;
        }
    
        self.table_set(cluster, ClusterValue::Last)?;
        Ok(first_cluster)
    }

    pub fn table_chain_extend(&self, cluster: u32, count: u32) -> Result<u32, FsErr> {
        let extend_cluster = self.table_chain_create(count)?;
        self.table_set(cluster, ClusterValue::Next(extend_cluster))?;
        Ok(extend_cluster)
    }
    /*
    fn sector(&self) -> Sector {
        Sector::new(self.io)
    }
    */
    pub fn cluster_to_sector(&self, cluster: u32) -> u32 {
        todo!();
        //self.data_area_first_sector + (cluster - self.root_cluster) * self.sectors_in_cluster
    }
}
//! Simple DWMAC Ethernet Driver Tutorial
//!
//! This is a simplified DWMAC driver designed for educational purposes.
//! It demonstrates the core concepts of ethernet driver development
//! without production-level complexity.

pub use dwmac_rs::DwmacHal;

use dwmac_rs::{DwmacNic as DwmacDriver, MAX_FRAME_SIZE, RX_DESC_COUNT, TX_DESC_COUNT};

use crate::{EthernetAddress, NetBufPtr, NetDriverOps};
use axdriver_base::{BaseDriverOps, DevError, DevResult, DeviceType};
use core::ptr::NonNull;
use core::sync::atomic::Ordering;
use log::*;

pub use dwmac_rs::PhysAddr;

extern crate alloc;

/// DWMAC device
pub struct DwmacNic<H: DwmacHal> {
    inner: DwmacDriver<H>,
    hwaddr: [u8; 6],
}

impl<H: DwmacHal> DwmacNic<H> {
    /// initialize dwmac driver
    pub fn init(base_addr: core::ptr::NonNull<u8>, mmio_size: usize) -> DevResult<Self> {
        info!("DwmacNic init @ {:#p}", base_addr.as_ptr());

        let inner = DwmacDriver::<H>::init(base_addr, mmio_size).unwrap();
        let hwaddr: [u8; 6] = inner.mac_addr;
        info!("Got DwmacNic HW address: {hwaddr:x?}");

        let dev = Self { inner, hwaddr };
        Ok(dev)
    }
}

unsafe impl<H: DwmacHal> Sync for DwmacNic<H> {}
unsafe impl<H: DwmacHal> Send for DwmacNic<H> {}

// Implement network driver traits
impl<H: DwmacHal> BaseDriverOps for DwmacNic<H> {
    fn device_type(&self) -> DeviceType {
        DeviceType::Net
    }

    fn device_name(&self) -> &str {
        "dwmac-5.2"
    }
}

impl<H: DwmacHal> NetDriverOps for DwmacNic<H> {
    fn mac_address(&self) -> EthernetAddress {
        EthernetAddress(self.hwaddr)
    }

    fn can_transmit(&self) -> bool {
        // self.inspect_dma_regs();
        // self.inspect_mtl_regs();
        self.inner.link_up.load(Ordering::Acquire) && self.inner.tx_ring.has_available_tx()
    }

    fn can_receive(&self) -> bool {
        if self.inner.rx_ring.has_completed_rx() {
            debug!("++ can_receive");
        }
        self.inner.link_up.load(Ordering::Acquire) && self.inner.rx_ring.has_completed_rx()
    }

    fn rx_queue_size(&self) -> usize {
        RX_DESC_COUNT
    }

    fn tx_queue_size(&self) -> usize {
        TX_DESC_COUNT
    }

    fn transmit(&mut self, tx_buf: NetBufPtr) -> DevResult {
        let ret = self.inner.transmit(tx_buf.packet());
        match ret {
            Ok(_) => Ok(()),
            Err(e) => {
                error!("transmit failed: {:?}", e);
                Err(DevError::Again)
            }
        }
    }

    /*
    fn clear_intr_status(&mut self) -> bool {
        self.inner.read_mac_intr_status();
        self.inner.clear_dma_intr_status()
    }
    */

    fn receive(&mut self) -> DevResult<NetBufPtr> {
        let rx = self.inner.receive();
        match rx {
            Err(_) => Err(DevError::Again),
            Ok(packet) => {
                log::debug!("received packet length {}", packet.len());
                let packet_len = packet.len();
                let rx_buf = NetBufPtr::new(
                    NonNull::new(packet.as_ptr() as *mut u8).unwrap(),
                    NonNull::new(packet.as_ptr() as *mut u8).unwrap(),
                    packet_len,
                );
                Ok(rx_buf)
            }
        }
    }

    fn recycle_rx_buffer(&mut self, rx_buf: NetBufPtr) -> DevResult {
        self.inner.rx_ring.mem_pool.free(rx_buf.buf_ptr());

        Ok(())
    }

    fn recycle_tx_buffers(&mut self) -> DevResult {
        self.inner.tx_ring.reclaim_tx_descriptors();
        Ok(())
    }

    fn alloc_tx_buffer(&mut self, size: usize) -> DevResult<NetBufPtr> {
        if size > MAX_FRAME_SIZE {
            return Err(DevError::InvalidParam);
        }

        let buf_ptr = self
            .inner
            .tx_ring
            .mem_pool
            .alloc()
            .ok_or(DevError::NoMemory)?;

        Ok(NetBufPtr::new(
            self.inner.tx_ring.mem_pool.base_ptr(),
            buf_ptr,
            size,
        ))
    }
}

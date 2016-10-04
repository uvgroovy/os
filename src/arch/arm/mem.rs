use core::slice;
use core::ops::{Index, IndexMut};
use core::cmp;
use core::ops;

use super::cpu;
use ::mem::FrameAllocator;

// contants are auto inlined: https://doc.rust-lang.org/book/const-and-static.html
pub const PAGE_SHIFT : usize = 12;
pub const PAGE_SIZE : usize = 1<<PAGE_SHIFT;
pub const PAGE_MASK : usize = PAGE_SIZE - 1;
// 4096 entries of 1MB each (=4gb address space). each entry is 4 bytes.
pub const L1TABLE_ENTRIES : usize = 4096; 
pub const L2TABLE_ENTRIES : usize = 256; 


pub const MB_SHIFT : usize = 20;
pub const MB_SIZE : usize = 1<<MB_SHIFT;
pub const MB_MASK : usize = MB_SIZE - 1;

pub struct LameFrameAllocator<'a> {

    nextfree : usize,
    max : usize,

    ranges : &'a [ops::Range<usize>]
    // TODO: add deallocated list
}

impl<'a> ::mem::FrameAllocator for LameFrameAllocator<'a> {

    fn allocate(&mut self, number: usize) -> Option<::mem::PhysicalAddress> {
        if self.nextfree >= self.max {
            return None;
        }

        let mut cur_free;
        let mut potentialNext;

        'outer: loop {
          cur_free = self.nextfree;

          potentialNext = cur_free + (number << PAGE_SHIFT);

          let curRange = cur_free .. potentialNext;

          for r in self.ranges {
            if (curRange.start < r.end) && (r.start < curRange.end) {
              self.nextfree = cmp::max(self.nextfree, r.end);
              continue 'outer;
            }
          }

            break;
        }

        self.nextfree = potentialNext;
        
        if self.nextfree > self.max {
            return None;
        }


        Some(::mem::PhysicalAddress(cur_free))
    }

    fn deallocate(&mut self, _ : ::mem::PhysicalAddress, _ : usize) {
        // make a short dealloc list
    }

}
impl<'a>  LameFrameAllocator<'a> {
    pub fn new(ranges : &'a [ops::Range<usize>], max_size : usize) -> LameFrameAllocator<'a> {
        LameFrameAllocator{
            max : max_size,
            nextfree: PAGE_SIZE, // don't allocate frame zero cause the vector table is there..
            ranges : ranges,
        }
    }

}

pub struct L1TableSectionDescriptor(u32);
pub struct L1TableCoarseDescriptor(u32);

pub struct L2TableDescriptor(u32);
pub struct L1TableDescriptor(u32);

pub struct FirstLevelTableDescriptor(u32);

// http://infocenter.arm.com/help/index.jsp?topic=/com.arm.doc.ddi0333h/Babifihd.html
const L2_CACHEABLE : u32 = 1 << 2;
const L2_BUFFERABLE : u32 = 1 << 2;
const L2_SHAREABLE  : u32 = 1 << 10;

const L2_NX  : u32 = 1;
const L2_XPAGE_TYPE  : u32 = 1 << 1;

// http://infocenter.arm.com/help/index.jsp?topic=/com.arm.doc.ddi0211k/Caceaije.html
// read write all:
const L2_AP_ALL_ACCESS  : u32 = 0b11 << 4;

// where we gonna map the virt table itself
const L1_VIRT_ADDRESS :  ::mem::VirtualAddress = ::mem::VirtualAddress(0xe000_0000);

impl L1TableDescriptor {
    fn new(physical_address_of_l2: ::mem::PhysicalAddress) -> L1TableDescriptor {

      if physical_address_of_l2.0 & PAGE_MASK != 0 {
        panic!("Can't map unaligned l2 frames")
      }

        let mut d : L1TableDescriptor = L1TableDescriptor(0);
        // 4kb page
        d.0 |= 1;

        // set permissions
        d.0 |= physical_address_of_l2.0 as u32;

        d
    }

  fn is_present(&self) -> bool {
      self.0 != 0
  }

  fn get_physical_address(&self) -> ::mem::PhysicalAddress {
    if ! self.is_present() {
      panic!("entry not present!")
    }
    ::mem::PhysicalAddress((self.0 as usize)& (!PAGE_MASK))
  }
}

impl L2TableDescriptor {
  fn new(physical_address_of_page: ::mem::PhysicalAddress) -> L2TableDescriptor {
    if (physical_address_of_page.0 & PAGE_MASK) != 0 {
      panic!("Can't map unaligned l2 frames")
    }

      let mut d : L2TableDescriptor = L2TableDescriptor(0);
      // 4kb page
      d.0 |= L2_XPAGE_TYPE;
      d.0 |= L2_CACHEABLE;
      d.0 |= L2_BUFFERABLE;
      d.0 |= L2_AP_ALL_ACCESS;

      // Only one cpu now.. no need to set shareable
      
      // set permissions

      d.0 |= physical_address_of_page.0 as u32;

      d
  }

  fn new_device(physical_address_of_page: ::mem::PhysicalAddress) -> L2TableDescriptor {
    if (physical_address_of_page.0 & PAGE_MASK) != 0 {
      panic!("Can't map unaligned l2 frames")
    }

      let mut d : L2TableDescriptor = L2TableDescriptor(0);
      // 4kb page
      d.0 |= L2_XPAGE_TYPE;
      d.0 |= L2_AP_ALL_ACCESS;

      // Only one cpu now.. no need to set shareable
      // set permissions
      d.0 |= physical_address_of_page.0 as u32;

      d
  }

  fn is_present(&self) -> bool {
      self.0 != 0
  }
  
  fn get_physical_address(&self) -> ::mem::PhysicalAddress {
    if ! self.is_present() {
      panic!("entry not present!")
    }
    ::mem::PhysicalAddress((self.0 as usize) & (!PAGE_MASK))
  }
}
// repr C might not be needed, but let's be on the safe side.
#[repr(C)]
pub struct L1Table {
  #[repr(C)]
   pub descriptors : &'static mut [L1TableDescriptor],
}
#[repr(C)]
pub struct L2Table {
   #[repr(C)]
   pub descriptors : &'static mut [L2TableDescriptor],
}


impl Index<usize> for L1Table {
    type Output = L1TableDescriptor;

    fn index(&self, index: usize) -> &L1TableDescriptor {
        &self.descriptors[index]
    }
}

impl IndexMut<usize> for L1Table {
    fn index_mut(&mut self, index: usize) -> &mut L1TableDescriptor {
        &mut self.descriptors[index]
    }
}


impl Index<usize> for L2Table {
    type Output = L2TableDescriptor;

    fn index(&self, index: usize) -> &L2TableDescriptor {
        &self.descriptors[index]
    }
}

impl IndexMut<usize> for L2Table {
    fn index_mut(&mut self, index: usize) -> &mut L2TableDescriptor {
        &mut self.descriptors[index]
    }
}

impl L1Table {

  pub fn map_page(&mut self, p : ::mem::PhysicalAddress, v : ::mem::VirtualAddress)  {
    let l1Index = v.0 >> MB_SHIFT;
    
    if self.descriptors[l1Index].0 == 0 {
      // allocate a coarse page table
      // set all the bits required
      // set the physical address in here
    }

    

//    let phy = self.descriptors[l1Index].get_physical_address();
    // find the virtual address to update
    
//CC ::mem::VirtualAddress(0)
  }

  pub fn set(&mut self, v : ::mem::VirtualAddress, desc : L1TableDescriptor) {
    let l1Index = v.0 >> MB_SHIFT;
    self.descriptors[l1Index] = desc;

  }
//    let phy = self.descriptors[l1Index].get_physical_address();
    // find the virtual address to update
    
//CC ::mem::VirtualAddress(0)
  

}

/*
The stub has provided us with an l1 table and an aligned buffer that we can use for l2 table that are identity mapped.

we are going to use those to the l2 buffer to map our new page tabled, initialize with the kernel,
stack and switch to it.

*/
pub struct MemLayout {
  pub kernel_start_phy : ::mem::PhysicalAddress,
  pub kernel_start_virt : ::mem::VirtualAddress,
  pub kernel_end_virt : ::mem::VirtualAddress,
  pub stack_phy : ::mem::PhysicalAddress,
  pub stack_virt : ::mem::VirtualAddress,
}


fn getInitFrames(fa : & mut ::mem::FrameAllocator) -> [::mem::PhysicalAddress;5]{
  const NUM_FRAMES : usize = 7; // guaranteed to have somthing aligned here..
  let mut freeFrames : [::mem::PhysicalAddress;7] = [::mem::PhysicalAddress(0);NUM_FRAMES];
  let pa = fa.allocate(freeFrames.len()).unwrap();

  // find out which one devides with 16k
  let l1StartFrame =(4 - ((pa.0 >> PAGE_SHIFT) & 0b11))& 0b11;

  for i in 0 .. NUM_FRAMES {
    let shiftedIndex = (i + l1StartFrame) % NUM_FRAMES;
    freeFrames[i] = pa.offset((shiftedIndex*PAGE_SIZE) as isize);
  }

  // don't need the last two..
  fa.deallocate(freeFrames[5], 1);
  fa.deallocate(freeFrames[6], 1);

  return [freeFrames[0],freeFrames[1],freeFrames[2],freeFrames[3],freeFrames[4]]
}

fn up(a : usize) -> usize {(a + PAGE_MASK) & (!PAGE_MASK)}

// TODO fix frame allocator to not use stub and stack.
pub fn init_page_table<'a>(l1table_identity : ::mem::VirtualAddress, l2table_identity : ::mem::VirtualAddress, ml : &MemLayout, fa : &'a mut ::mem::FrameAllocator) -> PageTable<'a> {
        let mut active_table = unsafe{ L1Table::from_virt_address(l1table_identity)};
        let mut l2 = unsafe{ L2Table::from_virt_address(l2table_identity)};

        // get seven frames, where the first four are contingous and aligned to 16kb:
        let freeFrames : [::mem::PhysicalAddress;5] = getInitFrames(fa);

        // first 4 frames are for l1 table (as they are on 16k boundery).
        // get the 5th frame to use as temporary coarse table

        // map our new page table to memory so we can write to it.
        active_table[L1_VIRT_ADDRESS.0 >> MB_SHIFT] =  L1TableDescriptor::new(::mem::PhysicalAddress(l2table_identity.0));

        // map the l1 page table
        l2[0] = L2TableDescriptor::new(freeFrames[0]);
        l2[1] = L2TableDescriptor::new(freeFrames[1]);
        l2[2] = L2TableDescriptor::new(freeFrames[2]);
        l2[3] = L2TableDescriptor::new(freeFrames[3]);
        // map one more frame so we can use map the table itself
        l2[4] = L2TableDescriptor::new(freeFrames[4]);

        let nextFreeL2Index = 5;

        // flush changes
        cpu::memory_write_barrier();
        cpu::invalidate_caches();
        cpu::invalidate_tlb();

        // our blank l1 and l2 mapped pages should be available now.
        let mut newl1 = unsafe{ L1Table::from_virt_address(L1_VIRT_ADDRESS)};
        let mut newl2 = unsafe{ L2Table::from_virt_address(L1_VIRT_ADDRESS.offset(4*PAGE_SIZE as isize))};

        // map the new map in itself in the same address.
        newl1[L1_VIRT_ADDRESS.0 >> MB_SHIFT] =  L1TableDescriptor::new(freeFrames[4]);
        newl2[0] = L2TableDescriptor::new(freeFrames[0]);
        newl2[1] = L2TableDescriptor::new(freeFrames[1]);
        newl2[2] = L2TableDescriptor::new(freeFrames[2]);
        newl2[3] = L2TableDescriptor::new(freeFrames[3]);
        newl2[4] = L2TableDescriptor::new(freeFrames[4]);

        // now when we will switch the page table, the page table itself will be available in the same place.
        

        // map the kernel in the new page table:
        let kernelSize = up((ml.kernel_end_virt - ml.kernel_start_virt) as usize);
        // mega bytes rounded up
        let nummb = ((kernelSize + MB_MASK) & (!MB_MASK)) >> MB_SHIFT;

        // for each meg:
        for i in 0..nummb {

          // get new frame
          let frame = fa.allocate(1).unwrap();

          // clean caches as we are about to remove stuff from memory
          // TODO: data sync barrier?
          cpu::memory_write_barrier();
          cpu::invalidate_caches();
          cpu::invalidate_tlb();

          // map the frame 
          l2[nextFreeL2Index] = L2TableDescriptor::new(frame);
        
          // flush changes
          cpu::memory_write_barrier();
          cpu::invalidate_caches();
          cpu::invalidate_tlb();
          
          // frame now available here:
          let frameAddress = ::mem::VirtualAddress(L1_VIRT_ADDRESS.0 + nextFreeL2Index*PAGE_SIZE);
          let mut currKernelL2 = unsafe{L2Table::from_virt_address(frameAddress)};
          // for each 4k block in the mb, map it in newframel2
          let curphy_start = ml.kernel_start_phy.offset((i << MB_SHIFT) as isize);
          // check that in the end we don't map a full MB
          let nextmb =  ml.kernel_start_phy.offset(((i+1) << MB_SHIFT) as isize);
          let curphy_end = if (i+1) == nummb {ml.kernel_start_phy.offset(kernelSize as isize)} else{nextmb};

          let mut l2loopindex = 0;
          for curFrame in  (curphy_start.0 .. curphy_end.0).step_by(PAGE_SIZE) {
            currKernelL2[l2loopindex] =  L2TableDescriptor::new(::mem::PhysicalAddress(curFrame));
            l2loopindex += 1;
          }
          for curFrame in  (curphy_end.0 .. nextmb.0).step_by(PAGE_SIZE) {
            currKernelL2[l2loopindex] =  L2TableDescriptor::new(::mem::PhysicalAddress(curFrame));
            l2loopindex += 1;
          }

          // add the l2 frame to the l2 map
          newl1[ (ml.kernel_start_virt.0 >> MB_SHIFT) + i ] =  L1TableDescriptor::new(frame);

        }
        // map the stack
        // get stack pointer
        let sp = ml.stack_virt.0 & (!PAGE_MASK);
        let spframe = ml.stack_phy.0 & (!PAGE_MASK);

        if ! newl1[sp >> MB_SHIFT].is_present() {
          let frame = fa.allocate(1).unwrap();
          newl1[sp >> MB_SHIFT] = L1TableDescriptor::new(frame);
        }
        // first, map the existing current stack
        let stackframe = newl1[sp >> MB_SHIFT].get_physical_address();

        // replaceing page - destroy old page in caches. TODO - is this needed.
        cpu::memory_write_barrier();
        cpu::invalidate_caches();
        cpu::invalidate_tlb();

        // temporary the l2 entry to memory
        l2[nextFreeL2Index] = L2TableDescriptor::new(stackframe);

        // flush changes
        cpu::memory_write_barrier();
        cpu::invalidate_caches();
        cpu::invalidate_tlb();

        let frameAddress = ::mem::VirtualAddress(L1_VIRT_ADDRESS.0 + nextFreeL2Index*PAGE_SIZE);
        let mut stackL2 = unsafe{L2Table::from_virt_address(frameAddress)};
        // TODO: set nx bit
        stackL2[(sp >>PAGE_SHIFT) & 0xFF] = L2TableDescriptor::new(::mem::PhysicalAddress(spframe));


        // turn on new mmu and free the stub memory
        // the kernel now has a page table with the l1 mapped to 
        // L1_VIRT_ADDRESS and l2 table that maps the virt table to L1_VIRT_ADDRESS mapping is at
        // L1_VIRT_ADDRESS + 5*PAGE_SIZE. it has five entries taken. so available from index #5
        // so once can use that 5th index to init new frames and place them in the page table.

        // when memory is freed, we need to find out the physical addresses so we can free them.
        // to do that we will need to map the l2 table, and temporary map it again. 
        // in read all the frames it points to and free them. 
        cpu::memory_write_barrier();
        // disable access checks for domain 0 
        // http://infocenter.arm.com/help/index.jsp?topic=/com.arm.doc.ddi0344k/I1001599.html
        //set domain 0 to what ever is in the table.
        cpu::write_domain_access_control_register(1);
        cpu::set_ttb0(freeFrames[0].0 as *const());
        cpu::invalidate_caches();
        cpu::invalidate_tlb();

        PageTable {
          descriptors : newl1,
          frameallocator : fa,
          tmpMap : newl2,
        }

}

pub struct PageTable<'a> {
   pub descriptors : L1Table,
   pub frameallocator : &'a mut ::mem::FrameAllocator,

   tmpMap : L2Table,
}


impl<'a> PageTable<'a> {

  fn map_single(&mut self, p : ::mem::PhysicalAddress, v : ::mem::VirtualAddress) {
      self.map_single_descriptor(L2TableDescriptor::new(p), v)
  }

  pub fn map_device(&mut self, p : ::mem::PhysicalAddress, v : ::mem::VirtualAddress) {
      self.map_single_descriptor(L2TableDescriptor::new_device(p), v)
  }

  // TODO: clear interrupts somewhere here? maybe not
  fn map_single_descriptor(&mut self, p : L2TableDescriptor, v : ::mem::VirtualAddress) {
    
    let l1Index = v.0 >> MB_SHIFT;
    // get physical addresss
    // temporary map it to here using the active page table
    if ! self.descriptors[l1Index].is_present() {
      let frame = self.frameallocator.allocate(1).unwrap();
      self.descriptors[l1Index] = L1TableDescriptor::new(frame);
    }

    let l2phy = self.descriptors[l1Index].get_physical_address();

    // SHOULD WE FLUSH CASHES HERE?
    
    // 0-3 are page table itself
    // 4 is the tmp map itself
    // 5 is free!
    const FREE_INDEX : usize = 5;
    self.tmpMap[FREE_INDEX] = L2TableDescriptor::new(l2phy);

    // TODO: find the frame for l2, and temporary map it..
    // and add teh mapping

    cpu::memory_write_barrier();
    cpu::invalidate_caches();
    cpu::invalidate_tlb();
    cpu::data_synchronization_barrier();
    
    let mapped_address = L1_VIRT_ADDRESS.offset((FREE_INDEX*PAGE_SIZE) as isize);
    // new frame.. zero it
    for i in (0..PAGE_SIZE).step_by(4) {
      unsafe{*(mapped_address.offset(i as isize).0 as *mut u32) = 0};
    }

    // frame now available here:
    let mut l2_for_phy = unsafe{ L2Table::from_virt_address(mapped_address)};

    let l2Index = (v.0 >> PAGE_SHIFT) & 0xFF;

    l2_for_phy[l2Index] = p;

    cpu::memory_write_barrier();
    cpu::invalidate_caches();
    cpu::invalidate_tlb();
    // page should be mapped now
  }
}
impl<'a> ::mem::MemoryMapper for PageTable<'a> {

  fn map(&mut self, p : ::mem::PhysicalAddress, v : ::mem::VirtualAddress, length : usize) {
    for i in (0 ..length).step_by(PAGE_SIZE) {
      self.map_single(p.offset(i as isize), v.offset(i as isize));
    }
  }

}

impl L1Table {

  unsafe fn from_virt_address(v : ::mem::VirtualAddress) -> L1Table {
    let slice : &'static mut [L1TableDescriptor] = unsafe{slice::from_raw_parts_mut(v.0 as *mut L1TableDescriptor, L1TABLE_ENTRIES)};
    L1Table{
      descriptors : slice
    }
  }


}


impl L2Table {

  unsafe fn from_virt_address(v : ::mem::VirtualAddress) -> L2Table {
    let slice : &'static mut [L2TableDescriptor] = unsafe{slice::from_raw_parts_mut(v.0 as *mut L2TableDescriptor, L2TABLE_ENTRIES)};
    L2Table{
      descriptors : slice
    }  
  }
}
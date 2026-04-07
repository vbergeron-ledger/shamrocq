use core::fmt;
#[cfg(feature = "stats")]
use shamrocq_bytecode::op;

#[cfg(feature = "stats")]
#[derive(Debug, Default, Clone)]
pub struct ArenaStats {
    pub peak_heap_bytes: usize,
    pub peak_stack_bytes: usize,

    pub alloc_count_ctor: u32,
    pub alloc_count_closure: u32,
    pub alloc_bytes_total: u32,

    pub reclaim_count: u32,
    pub reclaim_bytes_total: u32,

    pub gc_count: u32,
    pub gc_bytes_reclaimed: u32,
}

#[cfg(feature = "stats")]
#[derive(Debug, Clone)]
pub struct ExecStats {
    pub opcode_counts: [usize; 256],
    pub opcode_heap_words: [u32; 256],
    pub opcode_cycles: [u64; 256],
    pub peak_call_depth: u32,
}

#[cfg(feature = "stats")]
impl Default for ExecStats {
    fn default() -> Self {
        ExecStats {
            opcode_counts: [0; 256],
            opcode_heap_words: [0; 256],
            opcode_cycles: [0; 256],
            peak_call_depth: 0,
        }
    }
}

#[cfg(feature = "stats")]
#[derive(Debug, Clone)]
pub struct Stats {
    pub peak_heap_bytes: usize,
    pub peak_stack_bytes: usize,

    pub alloc_count_ctor: u32,
    pub alloc_count_closure: u32,
    pub alloc_bytes_total: u32,

    pub opcode_counts: [usize; 256],
    pub opcode_heap_words: [u32; 256],
    pub opcode_cycles: [u64; 256],
    pub peak_call_depth: u32,

    pub reclaim_count: u32,
    pub reclaim_bytes_total: u32,

    pub gc_count: u32,
    pub gc_bytes_reclaimed: u32,
}

#[cfg(feature = "stats")]
impl Stats {
    pub fn from(arena: &ArenaStats, exec: &ExecStats) -> Self {
        Stats {
            peak_heap_bytes: arena.peak_heap_bytes,
            peak_stack_bytes: arena.peak_stack_bytes,
            alloc_count_ctor: arena.alloc_count_ctor,
            alloc_count_closure: arena.alloc_count_closure,
            alloc_bytes_total: arena.alloc_bytes_total,
            reclaim_count: arena.reclaim_count,
            reclaim_bytes_total: arena.reclaim_bytes_total,
            gc_count: arena.gc_count,
            gc_bytes_reclaimed: arena.gc_bytes_reclaimed,
            opcode_counts: exec.opcode_counts,
            opcode_heap_words: exec.opcode_heap_words,
            opcode_cycles: exec.opcode_cycles,
            peak_call_depth: exec.peak_call_depth,
        }
    }

    pub fn instruction_count(&self) -> usize {
        self.opcode_counts.iter().sum()
    }
}

#[cfg(feature = "stats")]
impl fmt::Display for Stats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "  memory")?;
        writeln!(f, "    peak heap      {:>6} B", self.peak_heap_bytes)?;
        writeln!(f, "    peak stack     {:>6} B", self.peak_stack_bytes)?;
        writeln!(f, "  allocations")?;
        writeln!(f, "    ctors          {:>6}", self.alloc_count_ctor)?;
        writeln!(f, "    closures       {:>6}", self.alloc_count_closure)?;
        writeln!(f, "    total bytes    {:>6} B", self.alloc_bytes_total)?;
        let has_heap = self.opcode_heap_words.iter().any(|&w| w > 0);
        let has_cycles = self.opcode_cycles.iter().any(|&c| c > 0);
        writeln!(f, "  execution ({} instructions)", self.instruction_count())?;
        for (i, &count) in self.opcode_counts.iter().enumerate() {
            if count > 0 {
                write!(f, "    {:18} {:>6}", op::name(i as u8), count)?;
                if has_heap {
                    write!(f, " {:>8}w", self.opcode_heap_words[i])?;
                }
                if has_cycles {
                    write!(f, " {:>10}c", self.opcode_cycles[i])?;
                }
                writeln!(f)?;
            }
        }
        writeln!(f, "    peak depth     {:>6}", self.peak_call_depth)?;
        writeln!(f, "  reclaim")?;
        writeln!(f, "    count          {:>6}", self.reclaim_count)?;
        writeln!(f, "    bytes total    {:>6} B", self.reclaim_bytes_total)?;
        writeln!(f, "  gc")?;
        writeln!(f, "    collections    {:>6}", self.gc_count)?;
        write!(f,   "    bytes freed    {:>6} B", self.gc_bytes_reclaimed)
    }
}

#[derive(Debug, Clone)]
pub struct MemSnapshot {
    pub heap_bytes: usize,
    pub stack_bytes: usize,
    pub free_bytes: usize,
}

impl fmt::Display for MemSnapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "heap {:>6} B | stack {:>6} B | free {:>6} B",
            self.heap_bytes, self.stack_bytes, self.free_bytes,
        )
    }
}

macro_rules! stat {
    ($self:ident, $field:ident += $val:expr) => {
        #[cfg(feature = "stats")]
        {
            $self.stats.$field += $val;
        }
    };
    ($self:ident, $field:ident = max $val:expr) => {
        #[cfg(feature = "stats")]
        {
            let v = $val;
            if v > $self.stats.$field {
                $self.stats.$field = v;
            }
        }
    };
}

pub(crate) use stat;

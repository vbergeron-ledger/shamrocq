use core::fmt;

#[cfg(feature = "stats")]
#[derive(Debug, Default, Clone)]
pub struct Stats {
    pub peak_heap_bytes: usize,
    pub peak_stack_bytes: usize,

    pub alloc_count_ctor: u32,
    pub alloc_count_closure: u32,
    pub alloc_bytes_total: u32,

    pub exec_instruction_count: u64,
    pub exec_call_count: u32,
    pub exec_tail_call_count: u32,
    pub exec_match_count: u32,
    pub exec_peak_call_depth: u32,
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
        writeln!(f, "  execution")?;
        writeln!(f, "    instructions   {:>6}", self.exec_instruction_count)?;
        writeln!(f, "    calls          {:>6}", self.exec_call_count)?;
        writeln!(f, "    tail calls     {:>6}", self.exec_tail_call_count)?;
        writeln!(f, "    matches        {:>6}", self.exec_match_count)?;
        write!(f,   "    peak depth     {:>6}", self.exec_peak_call_depth)
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

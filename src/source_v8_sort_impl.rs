// V8's stable TimSort comparison schedule, adapted from Node v24.22.1
// deps/v8/third_party/v8/builtins/array-sort.tq. The schedule matters when
// a source subtraction comparator returns NaN (normalized by V8 to +0).
// Copyright (c) 2001, 2002, 2003, 2004, 2005, 2006, 2007, 2008, 2009, 2010,
// 2011, 2012, 2013, 2014, 2015, 2016, 2017, 2018 Python Software Foundation;
// All Rights Reserved.
// Copyright 2014, the V8 project authors. All rights reserved.
// Licenses: licenses/V8-PYTHON-TIMSORT.txt and licenses/V8-BSD.txt.
// Modification: safe Rust translation of the dense work-array comparison,
// run, gallop and merge schedule; no V8 object/accessor machinery or unsafe.
fn source_v8_stable_sort<T: Clone, E>(
    values: &mut [T],
    compare: impl FnMut(&T, &T) -> Result<core::cmp::Ordering, E>,
) -> Result<(), E> {
    let mut state = SourceV8Sort {
        rows: values.to_vec(),
        compare,
        runs: Vec::new(),
        min_gallop: 7,
        error: core::marker::PhantomData,
    };
    state.sort()?;
    values.clone_from_slice(&state.rows);
    Ok(())
}

struct SourceV8Sort<T, F, E> {
    rows: Vec<T>,
    compare: F,
    runs: Vec<(usize, usize)>,
    min_gallop: usize,
    error: core::marker::PhantomData<E>,
}

fn source_v8_gallop<T, E>(
    rows: &[T],
    key: &T,
    base: usize,
    length: usize,
    hint: usize,
    right: bool,
    compare: &mut impl FnMut(&T, &T) -> Result<core::cmp::Ordering, E>,
) -> Result<usize, E> {
    use core::cmp::Ordering::Less;
    let mut less = |index: usize| -> Result<bool, E> {
        if right {
            Ok(compare(key, &rows[index])? == Less)
        } else {
            Ok(compare(&rows[index], key)? != Less)
        }
    };
    let (mut last, mut offset) = (0isize, 1isize);
    if less(base + hint)? {
        let max = (hint + 1) as isize;
        while offset < max {
            if !less(base + hint - offset as usize)? {
                break;
            }
            last = offset;
            offset = 2 * offset + 1;
        }
        offset = offset.min(max);
        (last, offset) = (hint as isize - offset, hint as isize - last);
    } else {
        let max = (length - hint) as isize;
        while offset < max {
            if less(base + hint + offset as usize)? {
                break;
            }
            last = offset;
            offset = 2 * offset + 1;
        }
        offset = offset.min(max);
        last += hint as isize;
        offset += hint as isize;
    }
    last += 1;
    while last < offset {
        let mid = last + (offset - last) / 2;
        if less(base + mid as usize)? {
            offset = mid;
        } else {
            last = mid + 1;
        }
    }
    Ok(offset as usize)
}

impl<T: Clone, E, F: FnMut(&T, &T) -> Result<core::cmp::Ordering, E>> SourceV8Sort<T, F, E> {
    fn compare_rows(&mut self, a: usize, b: usize) -> Result<core::cmp::Ordering, E> {
        (self.compare)(&self.rows[a], &self.rows[b])
    }

    fn copy(&mut self, from: usize, to: usize, length: usize) {
        let copied = self.rows[from..from + length].to_vec();
        self.rows[to..to + length].clone_from_slice(&copied);
    }

    fn count_run(&mut self, low: usize, high: usize) -> Result<usize, E> {
        use core::cmp::Ordering::Less;
        if low + 1 == high {
            return Ok(1);
        }
        let descending = self.compare_rows(low + 1, low)? == Less;
        let mut length = 2;
        while low + length < high {
            let order = self.compare_rows(low + length, low + length - 1)?;
            if if descending {
                order != Less
            } else {
                order == Less
            } {
                break;
            }
            length += 1;
        }
        if descending {
            self.rows[low..low + length].reverse();
        }
        Ok(length)
    }

    fn insertion(&mut self, low: usize, start: usize, high: usize) -> Result<(), E> {
        for at in start.max(low + 1)..high {
            let pivot = self.rows[at].clone();
            let (mut left, mut right) = (low, at);
            while left < right {
                let mid = left + (right - left) / 2;
                if (self.compare)(&pivot, &self.rows[mid])? == core::cmp::Ordering::Less {
                    right = mid;
                } else {
                    left = mid + 1;
                }
            }
            self.rows[left..=at].rotate_right(1);
        }
        Ok(())
    }

    fn merge_at(&mut self, i: usize) -> Result<(), E> {
        let (mut base_a, mut length_a) = self.runs[i];
        let (base_b, mut length_b) = self.runs[i + 1];
        self.runs[i].1 = length_a + length_b;
        self.runs.remove(i + 1);
        let k = source_v8_gallop(
            &self.rows,
            &self.rows[base_b],
            base_a,
            length_a,
            0,
            true,
            &mut self.compare,
        )?;
        base_a += k;
        length_a -= k;
        if length_a == 0 {
            return Ok(());
        }
        length_b = source_v8_gallop(
            &self.rows,
            &self.rows[base_a + length_a - 1],
            base_b,
            length_b,
            length_b - 1,
            false,
            &mut self.compare,
        )?;
        if length_b == 0 {
            return Ok(());
        }
        if length_a <= length_b {
            self.merge_low(base_a, length_a, base_b, length_b)
        } else {
            self.merge_high(base_a, length_a, base_b, length_b)
        }
    }

    fn merge_low(
        &mut self,
        base_a: usize,
        mut a: usize,
        base_b: usize,
        mut b: usize,
    ) -> Result<(), E> {
        use core::cmp::Ordering::Less;
        let temp = self.rows[base_a..base_a + a].to_vec();
        let (mut dest, mut ca, mut cb) = (base_a, 0, base_b);
        self.rows[dest] = self.rows[cb].clone();
        dest += 1;
        cb += 1;
        b -= 1;
        let mut min = self.min_gallop;
        'merge: while b != 0 && a != 1 {
            let (mut wins_a, mut wins_b) = (0, 0);
            loop {
                if (self.compare)(&self.rows[cb], &temp[ca])? == Less {
                    self.rows[dest] = self.rows[cb].clone();
                    dest += 1;
                    cb += 1;
                    b -= 1;
                    wins_b += 1;
                    wins_a = 0;
                    if b == 0 {
                        break 'merge;
                    }
                    if wins_b >= min {
                        break;
                    }
                } else {
                    self.rows[dest] = temp[ca].clone();
                    dest += 1;
                    ca += 1;
                    a -= 1;
                    wins_a += 1;
                    wins_b = 0;
                    if a == 1 {
                        break 'merge;
                    }
                    if wins_a >= min {
                        break;
                    }
                }
            }
            min += 1;
            let mut first = true;
            while wins_a >= 7 || wins_b >= 7 || first {
                first = false;
                min = 1.max(min - 1);
                self.min_gallop = min;
                wins_a =
                    source_v8_gallop(&temp, &self.rows[cb], ca, a, 0, true, &mut self.compare)?;
                if wins_a > 0 {
                    self.rows[dest..dest + wins_a].clone_from_slice(&temp[ca..ca + wins_a]);
                    dest += wins_a;
                    ca += wins_a;
                    a -= wins_a;
                    if a <= 1 {
                        break 'merge;
                    }
                }
                self.rows[dest] = self.rows[cb].clone();
                dest += 1;
                cb += 1;
                b -= 1;
                if b == 0 {
                    break 'merge;
                }
                wins_b =
                    source_v8_gallop(&self.rows, &temp[ca], cb, b, 0, false, &mut self.compare)?;
                if wins_b > 0 {
                    self.copy(cb, dest, wins_b);
                    dest += wins_b;
                    cb += wins_b;
                    b -= wins_b;
                    if b == 0 {
                        break 'merge;
                    }
                }
                self.rows[dest] = temp[ca].clone();
                dest += 1;
                ca += 1;
                a -= 1;
                if a == 1 {
                    break 'merge;
                }
            }
            min += 1;
            self.min_gallop = min;
        }
        if a == 1 && b > 0 {
            self.copy(cb, dest, b);
            self.rows[dest + b] = temp[ca].clone();
        } else if a > 0 {
            self.rows[dest..dest + a].clone_from_slice(&temp[ca..ca + a]);
        }
        Ok(())
    }

    fn merge_high(
        &mut self,
        base_a: usize,
        mut a: usize,
        base_b: usize,
        mut b: usize,
    ) -> Result<(), E> {
        use core::cmp::Ordering::Less;
        let temp = self.rows[base_b..base_b + b].to_vec();
        // One-past cursors avoid unsigned underflow at an exhausted left edge.
        let (mut dest, mut ca, mut cb) = (base_b + b, base_a + a, b);
        dest -= 1;
        ca -= 1;
        self.rows[dest] = self.rows[ca].clone();
        a -= 1;
        let mut min = self.min_gallop;
        'merge: while a != 0 && b != 1 {
            let (mut wins_a, mut wins_b) = (0, 0);
            loop {
                if (self.compare)(&temp[cb - 1], &self.rows[ca - 1])? == Less {
                    dest -= 1;
                    ca -= 1;
                    self.rows[dest] = self.rows[ca].clone();
                    a -= 1;
                    wins_a += 1;
                    wins_b = 0;
                    if a == 0 {
                        break 'merge;
                    }
                    if wins_a >= min {
                        break;
                    }
                } else {
                    dest -= 1;
                    cb -= 1;
                    self.rows[dest] = temp[cb].clone();
                    b -= 1;
                    wins_b += 1;
                    wins_a = 0;
                    if b == 1 {
                        break 'merge;
                    }
                    if wins_b >= min {
                        break;
                    }
                }
            }
            min += 1;
            let mut first = true;
            while wins_a >= 7 || wins_b >= 7 || first {
                first = false;
                min = 1.max(min - 1);
                self.min_gallop = min;
                let k = source_v8_gallop(
                    &self.rows,
                    &temp[cb - 1],
                    base_a,
                    a,
                    a - 1,
                    true,
                    &mut self.compare,
                )?;
                wins_a = a - k;
                if wins_a > 0 {
                    dest -= wins_a;
                    ca -= wins_a;
                    self.copy(ca, dest, wins_a);
                    a -= wins_a;
                    if a == 0 {
                        break 'merge;
                    }
                }
                dest -= 1;
                cb -= 1;
                self.rows[dest] = temp[cb].clone();
                b -= 1;
                if b == 1 {
                    break 'merge;
                }
                let k = source_v8_gallop(
                    &temp,
                    &self.rows[ca - 1],
                    0,
                    b,
                    b - 1,
                    false,
                    &mut self.compare,
                )?;
                wins_b = b - k;
                if wins_b > 0 {
                    dest -= wins_b;
                    cb -= wins_b;
                    self.rows[dest..dest + wins_b].clone_from_slice(&temp[cb..cb + wins_b]);
                    b -= wins_b;
                    if b <= 1 {
                        break 'merge;
                    }
                }
                dest -= 1;
                ca -= 1;
                self.rows[dest] = self.rows[ca].clone();
                a -= 1;
                if a == 0 {
                    break 'merge;
                }
            }
            min += 1;
            self.min_gallop = min;
        }
        if b == 1 && a > 0 {
            dest -= a;
            ca -= a;
            self.copy(ca, dest, a);
            self.rows[dest - 1] = temp[cb - 1].clone();
        } else if b > 0 {
            self.rows[dest - b..dest].clone_from_slice(&temp[..b]);
        }
        Ok(())
    }

    fn collapse(&mut self, force: bool) -> Result<(), E> {
        while self.runs.len() > 1 {
            let mut n = self.runs.len() - 2;
            if force {
                if n > 0 && self.runs[n - 1].1 < self.runs[n + 1].1 {
                    n -= 1;
                }
            } else if (n >= 1 && self.runs[n - 1].1 <= self.runs[n].1 + self.runs[n + 1].1)
                || (n >= 2 && self.runs[n - 2].1 <= self.runs[n - 1].1 + self.runs[n].1)
            {
                if self.runs[n - 1].1 < self.runs[n + 1].1 {
                    n -= 1;
                }
            } else if self.runs[n].1 > self.runs[n + 1].1 {
                break;
            }
            self.merge_at(n)?;
        }
        Ok(())
    }

    fn sort(&mut self) -> Result<(), E> {
        if self.rows.len() < 2 {
            return Ok(());
        }
        let mut n = self.rows.len();
        let mut r = 0;
        while n >= 64 {
            r |= n & 1;
            n >>= 1;
        }
        let min = n + r;
        let mut low = 0;
        while low < self.rows.len() {
            let mut length = self.count_run(low, self.rows.len())?;
            if length < min {
                let forced = min.min(self.rows.len() - low);
                self.insertion(low, low + length, low + forced)?;
                length = forced;
            }
            self.runs.push((low, length));
            self.collapse(false)?;
            low += length;
        }
        self.collapse(true)
    }
}

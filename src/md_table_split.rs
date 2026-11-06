// SPDX-License-Identifier: AGPL-3.0-or-later

use ranger::prelude::*;
use ranger::cell::RefCell;
use std::rc::Rc;


/// ============================================================================
/// MdTableSplit.rgr — where a table too tall for its slide is cut.
/// ============================================================================
///
///   npm run markdown:tablesplit:test
///
/// A table is a list of ROW GROUPS (one row each, or the rows a `rowspan`
/// ties together) under a header that is drawn again at the top of every
/// slide the table continues on. This decides which slide each group goes
/// to; it knows heights and nothing else, so the layout measures and draws
/// and this is tested without either.
///
/// Filling each slide to the brim leaves the last one with whatever is left
/// — sixteen rows came out 6 + 8 + 2. The number of slides is the greedy one,
/// the fewest there can be; with `balance` the rows are then spread over that
/// many slides as evenly as their heights allow, so the same table is 5 + 6
/// + 5. The first slide may have less room (the table starts under text) and
/// gets fewer rows; it never gets more than fit.
///
/// `greedyTail` is what the last slide would hold without balancing. A
/// balanced two-slide table whose greedy tail is short is the case the
/// slide shrinker exists for (a few rows over is better set a little smaller
/// than given a slide), and it reads this to judge the spill by what it
/// would have been rather than by the half a table balancing moved over.
/// ============================================================================
#[derive(Clone)]
pub struct MdTablePlan {
    /// per group: 0 the slide the table starts on, 1 the next, …
    pub slideOf: Vec<i64>,
    /// how many slides the table touches, the first counted even when the
    /// table had to start on the next one
    pub slides: i64,
    /// the height of the body rows on the last slide, balanced and greedy
    pub tail: f64,
    pub greedyTail: f64,
}

impl MdTablePlan {
    pub fn new() -> Self {
        Self {
            slideOf: Vec::new(),
            slides: 1,
            tail: 0.0,
            greedyTail: 0.0,
        }
    }
}

#[derive(Clone)]
pub struct MdTableSplit {
}

impl MdTableSplit {
    pub fn new() -> Self {
        Self {
        }
    }
    /// groups: each group's height. headH: the header drawn on every slide the
    /// table is on. firstRoom: what is left on the slide the table starts on;
    /// nextRoom: what a continuation slide has under its title.
    pub fn plan(groups: &[f64], head_h: f64, first_room: f64, next_room: f64, balance: bool) -> Rc<RefCell<MdTablePlan>> {
        let mut greedy: Rc<RefCell<MdTablePlan>> = MdTableSplit::pack(
            groups,
            head_h,
            first_room,
            next_room,
            1000000000000.0
        );
        let greedyTail_v = greedy.borrow().tail;
        greedy.borrow_mut().greedyTail = greedyTail_v;
        if greedy.borrow().slides < 2 || !balance {
            return greedy;
        }
        let mut _tmp_2 = greedy.borrow().slides.clone();
        let mut even: Rc<RefCell<MdTablePlan>> = MdTableSplit::evenly(
            groups,
            head_h,
            first_room,
            next_room,
            _tmp_2
        );
        if even.borrow().slides == greedy.borrow().slides {
            even.borrow_mut().greedyTail = greedy.borrow().tail;
            return even;
        }
        // heights too uneven for the even spread: the smallest per-slide cap
        // that still keeps the greedy count, found by halving
        let mut lo: f64 = 0.0;
        let mut hi: f64 = next_room - head_h;
        let mut g: i64 = 0;
        while g < (groups.len() as i64) {
            if groups[g as usize] > lo {
                lo = groups[g as usize];
            }
            g += 1;
        }
        if hi < lo {
            return greedy;
        }
        let mut best: Rc<RefCell<MdTablePlan>> = greedy.clone();
        let mut step: i64 = 0;
        while step < 40 {
            let mid: f64 = (lo + hi) / 2.0;
            let mut p: Rc<RefCell<MdTablePlan>> = MdTableSplit::pack(
                groups,
                head_h,
                first_room,
                next_room,
                mid
            );
            if p.borrow().slides <= greedy.borrow().slides {
                best = p.clone();
                hi = mid;
            } else {
                lo = mid;
            }
            step += 1;
        }
        best.borrow_mut().greedyTail = greedy.borrow().tail;
        best
    }
    /// Fill each slide up to min(its room, cap), in order.
    pub fn pack(groups: &[f64], head_h: f64, first_room: f64, next_room: f64, cap: f64) -> Rc<RefCell<MdTablePlan>> {
        let mut out: Rc<RefCell<MdTablePlan>> = Rc::new(RefCell::new(MdTablePlan::new()));
        let mut slide: i64 = 0;
        let mut sum: f64 = 0.0;
        let mut i: i64 = 0;
        let n: i64 = groups.len() as i64;
        while i < n {
            let gh: f64 = groups[i as usize];
            let mut limit: f64 = MdTableSplit::roomOf(slide, head_h, first_room, next_room);
            if cap < limit {
                limit = cap;
            }
            if sum + gh > limit + 0.01 {
                if sum > 0.0 {
                    slide += 1;
                    sum = 0.0;
                } else {
                    // nothing on this slide yet and the group does not fit:
                    // the first slide is given up when a slide of its own
                    // would hold it, any other takes it and is drawn cut
                    if slide == 0 && gh <= (next_room - head_h) + 0.01 {
                        if first_room < next_room - 0.5 {
                            slide = 1;
                        }
                    }
                }
            }
            out.borrow_mut().slideOf.push(slide);
            sum += gh;
            i += 1;
        }
        out.borrow_mut().slides = slide + 1;
        out.borrow_mut().tail = sum;
        out
    }
    /// `k` slides, each holding as near the same height as the rest as group
    /// boundaries allow, and never more than its room.
    pub fn evenly(groups: &[f64], head_h: f64, first_room: f64, next_room: f64, k: i64) -> Rc<RefCell<MdTablePlan>> {
        let mut out: Rc<RefCell<MdTablePlan>> = Rc::new(RefCell::new(MdTablePlan::new()));
        let n: i64 = groups.len() as i64;
        let mut left: f64 = 0.0;
        let mut i: i64 = 0;
        while i < n {
            left += groups[i as usize];
            i += 1;
        }
        let mut slide: i64 = 0;
        let mut sum: f64 = 0.0;
        let mut target: f64 = left / (k as f64);
        i = 0;
        while i < n {
            let gh: f64 = groups[i as usize];
            let room: f64 = MdTableSplit::roomOf(slide, head_h, first_room, next_room);
            let fits: bool = sum + gh <= room + 0.01;
            let nearer: bool = MdTableSplit::dist(sum + gh, target) <= MdTableSplit::dist(sum, target) + 0.01;
            let last_slide: bool = slide >= k - 1;
            if sum > 0.0 && ((!fits || !nearer) && !last_slide) {
                slide += 1;
                sum = 0.0;
                let slides_left: i64 = k - slide;
                target = left / (slides_left as f64);
            } else {
                if sum <= 0.0 && !fits {
                    let own: bool = gh <= (next_room - head_h) + 0.01;
                    if (slide == 0 && own) && (first_room < next_room - 0.5 && k > 1) {
                        // the first slide has no room for even one group
                        slide = 1;
                        target = left / ((k - 1) as f64);
                    }
                }
            }
            out.borrow_mut().slideOf.push(slide);
            sum += gh;
            left -= gh;
            i += 1;
        }
        // the last slide took what was left: over its room means the spread
        // failed, which the caller sees as one slide more
        out.borrow_mut().slides = slide + 1;
        out.borrow_mut().tail = sum;
        let last_room: f64 = MdTableSplit::roomOf(slide, head_h, first_room, next_room);
        if sum > last_room + 0.01 {
            let slides_v = out.borrow().slides + 1;
            out.borrow_mut().slides = slides_v;
        }
        out
    }
    pub fn roomOf(slide: i64, head_h: f64, first_room: f64, next_room: f64) -> f64 {
        if slide == 0 {
            return first_room - head_h;
        }
        next_room - head_h
    }
    pub fn dist(a: f64, b: f64) -> f64 {
        if a > b {
            return a - b;
        }
        b - a
    }
}

//! Homes into nodes. Recursive bisection along whichever way a group is widest, so nodes come out compact.

use schema::common::LonLat;
use schema::region::Home;

use crate::geo::dist_ft;

/// One group's homes, by index into the map's list.
pub type Group = Vec<usize>;

/// Splits `group` in two along its long axis, keeping the order stable for determinism.
pub fn bisect(homes: &[Home], group: &Group) -> (Group, Group) {
    let at = |i: usize| homes[i].geo;
    let (mut lon_lo, mut lon_hi, mut lat_lo, mut lat_hi) = (
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
    );
    for &i in group {
        let g = at(i);
        lon_lo = lon_lo.min(g.lon_deg);
        lon_hi = lon_hi.max(g.lon_deg);
        lat_lo = lat_lo.min(g.lat_deg);
        lat_hi = lat_hi.max(g.lat_deg);
    }
    let wide = dist_ft(
        LonLat {
            lon_deg: lon_lo,
            lat_deg: lat_lo,
        },
        LonLat {
            lon_deg: lon_hi,
            lat_deg: lat_lo,
        },
    ) >= dist_ft(
        LonLat {
            lon_deg: lon_lo,
            lat_deg: lat_lo,
        },
        LonLat {
            lon_deg: lon_lo,
            lat_deg: lat_hi,
        },
    );
    let mut sorted = group.clone();
    sorted.sort_by(|a, b| {
        let (x, y) = (at(*a), at(*b));
        let order = if wide {
            x.lon_deg.total_cmp(&y.lon_deg)
        } else {
            x.lat_deg.total_cmp(&y.lat_deg)
        };
        order.then(a.cmp(b))
    });
    let upper = sorted.split_off(sorted.len() / 2);
    (sorted, upper)
}

/// Bisects until every group is at or under `target`.
pub fn partition(homes: &[Home], target: usize) -> Vec<Group> {
    let mut done = Vec::new();
    let mut work: Vec<Group> = vec![(0..homes.len()).collect()];
    while let Some(group) = work.pop() {
        if group.len() <= target.max(1) {
            if !group.is_empty() {
                done.push(group);
            }
            continue;
        }
        let (a, b) = bisect(homes, &group);
        work.push(b);
        work.push(a);
    }
    done
}

/// With pins, each pin takes the homes nearest it, and the rest partition around them.
pub fn partition_with_pins(homes: &[Home], pins: &[LonLat], target: usize) -> (Vec<Group>, Group) {
    if pins.is_empty() {
        return (Vec::new(), (0..homes.len()).collect());
    }
    let mut order: Vec<(f64, usize, usize)> = Vec::new();
    for (i, home) in homes.iter().enumerate() {
        for (p, pin) in pins.iter().enumerate() {
            order.push((dist_ft(home.geo, *pin), i, p));
        }
    }
    order.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
    let mut groups: Vec<Group> = vec![Vec::new(); pins.len()];
    let mut taken = vec![false; homes.len()];
    for (_, home, pin) in order {
        if !taken[home] && groups[pin].len() < target {
            groups[pin].push(home);
            taken[home] = true;
        }
    }
    for group in &mut groups {
        group.sort_unstable();
    }
    let rest = (0..homes.len()).filter(|i| !taken[*i]).collect();
    (groups, rest)
}

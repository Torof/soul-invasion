//! Fixed-point numbers and vectors on the unit sphere (D24: integers only):
//! sines from a Taylor series, latitude and longitude, distances in km.

/// The fixed-point unit: unit vectors have this length, angles are in
/// millionths of a radian.
pub const ONE: i64 = 1_000_000;

/// The planet's radius, km: the Earth's (the lead's call, 26 Nov).
pub const RADIUS_KM: i64 = 6_371;

/// A vector, each component scaled by [`ONE`].
pub type V3 = [i64; 3];

pub(crate) const PI: i64 = 3_141_593;
pub(crate) const HALF_PI: i64 = 1_570_796;
pub(crate) const TAU: i64 = 6_283_185;

/// The sine of an angle in millionths of a radian, scaled by [`ONE`].
pub fn sin(a: i64) -> i64 {
    let mut x = a.rem_euclid(TAU);
    if x > PI {
        x -= TAU;
    }
    if x > HALF_PI {
        x = PI - x;
    } else if x < -HALF_PI {
        x = -PI - x;
    }
    let x = i128::from(x);
    let one = i128::from(ONE);
    let mut term = x;
    let mut sum = x;
    for k in 1..9_i128 {
        term = -term * x / one * x / one / ((2 * k) * (2 * k + 1));
        sum += term;
    }
    sum as i64
}

/// The cosine, as [`sin`].
pub fn cos(a: i64) -> i64 {
    sin(a + HALF_PI)
}

/// An angle in tenths of a degree, in millionths of a radian.
pub fn tenths(t: i64) -> i64 {
    t * PI / 1800
}

pub(crate) fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

pub(crate) fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

/// `a` times `k / ONE`.
pub(crate) fn scale(a: V3, k: i64) -> V3 {
    let f = |v: i64| (i128::from(v) * i128::from(k) / i128::from(ONE)) as i64;
    [f(a[0]), f(a[1]), f(a[2])]
}

/// The dot product, scaled by [`ONE`].
pub fn dot(a: V3, b: V3) -> i64 {
    let s: i128 = (0..3).map(|i| i128::from(a[i]) * i128::from(b[i])).sum();
    (s / i128::from(ONE)) as i64
}

pub(crate) fn cross(a: V3, b: V3) -> V3 {
    let c = |x: i64, y: i64, z: i64, w: i64| {
        ((i128::from(x) * i128::from(y) - i128::from(z) * i128::from(w)) / i128::from(ONE)) as i64
    };
    [
        c(a[1], b[2], a[2], b[1]),
        c(a[2], b[0], a[0], b[2]),
        c(a[0], b[1], a[1], b[0]),
    ]
}

pub(crate) fn norm(a: V3) -> i64 {
    let s: i128 = a.iter().map(|&v| i128::from(v) * i128::from(v)).sum();
    s.cast_unsigned().isqrt() as i64
}

/// `a` brought to length [`ONE`] (a null vector stays null).
pub fn normalize(a: V3) -> V3 {
    let n = norm(a);
    if n == 0 {
        return a;
    }
    let f = |v: i64| (i128::from(v) * i128::from(ONE) / i128::from(n)) as i64;
    [f(a[0]), f(a[1]), f(a[2])]
}

/// The latitude of a direction, in tenths of a degree (north positive).
pub fn latitude(v: V3) -> i64 {
    let z = i128::from(v[2]) * i128::from(ONE) / i128::from(norm(v).max(1));
    let z = z as i64;
    let (mut lo, mut hi) = (-900_i64, 900_i64);
    while lo < hi {
        let mid = (lo + hi + 1).div_euclid(2);
        if sin(tenths(mid)) <= z {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    lo
}

/// The angle of `(x, y)` from the x axis, in tenths of a degree, in
/// −1800..=1800 (an integer `atan2`).
pub fn angle_of(x: i64, y: i64) -> i64 {
    let r = (i128::from(x) * i128::from(x) + i128::from(y) * i128::from(y))
        .cast_unsigned()
        .isqrt() as i128;
    if r == 0 {
        return 0;
    }
    let c = (i128::from(x) * i128::from(ONE) / r) as i64;
    let (mut lo, mut hi) = (0_i64, 1800_i64);
    while lo < hi {
        let mid = (lo + hi + 1) / 2;
        if cos(tenths(mid)) >= c {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    if y < 0 { -lo } else { lo }
}

/// The longitude of a direction, in tenths of a degree (east positive).
pub fn longitude(v: V3) -> i64 {
    angle_of(v[0], v[1])
}

/// The direction at a latitude and longitude, both in tenths of a degree.
pub fn from_lat_lon(lat: i64, lon: i64) -> V3 {
    let (cl, sl) = (cos(tenths(lat)), sin(tenths(lat)));
    let (co, so) = (cos(tenths(lon)), sin(tenths(lon)));
    [cl * co / ONE, cl * so / ONE, sl]
}

/// The unit vector pointing east at `v` (tangent to the sphere).
pub fn east_at(v: V3) -> V3 {
    let e = normalize([-v[1], v[0], 0]);
    if e == [0, 0, 0] { [0, ONE, 0] } else { e }
}

/// The unit vector pointing north at `v`, given its east.
pub fn north_at(v: V3, east: V3) -> V3 {
    normalize(cross(v, east))
}

/// `v` moved `km` along the tangent direction `dir`.
pub fn step(v: V3, dir: V3, km: i64) -> V3 {
    normalize(add(v, scale(dir, km * ONE / RADIUS_KM)))
}

/// The straight distance between two points of the sphere, km (the
/// chord: within 1 % of the great circle below 1,500 km).
pub fn km_between(a: V3, b: V3) -> i64 {
    norm(sub(a, b)) * RADIUS_KM / ONE
}

/// A bump of half-width `half` around zero, per mille: 1,000 at 0, 0 at
/// `±half` and beyond, smooth at both ends (`(1 − t²)²`).
pub fn bump(x: i64, half: i64) -> i64 {
    if half <= 0 {
        return 0;
    }
    let t = x * 1000 / half;
    if t.abs() >= 1000 {
        return 0;
    }
    let q = 1000 - t * t / 1000;
    q * q / 1000
}

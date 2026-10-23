//! Environment as a pure function of time: `Cosmos::sample(t)` with t in days.
//! Stateless on purpose: any scale can query any instant, the viewer can scrub,
//! and a surrogate sees the exact same drivers as the detailed model.
//!
//! Change is the point. Every driver has its own period so their beats never
//! repeat cleanly: day, two moons (spring/neap tides), year (seasons via tilt,
//! irradiance via eccentricity), star activity cycle, flares, a pulsar, a
//! wobbling binary, galactic-arm cosmic rays, rare supernovae.

use serde::{Deserialize, Serialize};
use std::f64::consts::TAU;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CosmosParams {
    pub seed: u64,
    /// Days per orbit. Short (vs Earth 365) so local runs see several seasons.
    pub year_days: f64,
    pub eccentricity: f64,
    /// Axial tilt, radians. Drives seasons.
    pub obliquity: f64,
    /// Latitude of the simulated site, radians.
    pub latitude: f64,
    /// Sidereal periods of the two moons, days.
    pub moon_periods: [f64; 2],
    /// Tide amplitude of each moon, then the sun. Level units (0..1 of beach).
    pub tide_amps: [f64; 3],
    pub mean_sea_level: f64,
    /// Star activity cycle, years.
    pub star_cycle_years: f64,
    /// Flares per day at activity maximum.
    pub flare_rate_max: f64,
    /// Supernovae per day (far sources, rare).
    pub supernova_rate: f64,
    pub pulsar_period_days: f64,
    pub binary_period_days: f64,
    /// Galactic-arm crossing period for cosmic-ray background, days.
    pub galactic_period_days: f64,
    /// Planet radius: Earth's. Sets distances between latitudes (coast site spacing).
    #[serde(default = "earth_radius")]
    pub planet_radius_km: f64,
}

fn earth_radius() -> f64 {
    EARTH_RADIUS_KM
}

pub const EARTH_RADIUS_KM: f64 = 6371.0;

impl CosmosParams {
    /// Ground distance between two latitudes on one meridian, km.
    pub fn meridian_km(&self, lat_a: f64, lat_b: f64) -> f64 {
        self.planet_radius_km * (lat_a - lat_b).abs()
    }
}

impl Default for CosmosParams {
    fn default() -> Self {
        Self {
            seed: 1,
            year_days: 96.0,
            eccentricity: 0.08,
            obliquity: 0.42,
            latitude: 0.55,
            moon_periods: [18.3, 5.1],
            tide_amps: [0.16, 0.05, 0.07],
            mean_sea_level: 0.5,
            star_cycle_years: 11.0,
            flare_rate_max: 0.6,
            supernova_rate: 1.0 / 3000.0,
            pulsar_period_days: 0.0137,
            binary_period_days: 41.0,
            galactic_period_days: 2400.0,
            planet_radius_km: EARTH_RADIUS_KM,
        }
    }
}

/// Everything a site on the planet feels at one instant.
/// Fluxes are relative: 1.0 = typical noon / typical background.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct EnvSample {
    pub t_days: f64,
    /// 0..1 through the local day; 0 = midnight.
    pub day_phase: f64,
    /// 0..1 through the year; 0 = northern spring equinox.
    pub year_phase: f64,
    pub year: f64,
    /// Sine of sun elevation; <0 is night.
    pub sun_height: f64,
    /// Orbit distance in units of the semi-major axis.
    pub orbit_distance: f64,
    pub cloud: f64,
    pub visible: f64,
    pub uv: f64,
    pub radio: f64,
    pub cosmic: f64,
    pub temperature_k: f64,
    /// Thermal energy, units of kT at 300 K.
    pub kt: f64,
    /// Water level 0..1 along the beach slope.
    pub tide_level: f64,
    /// d(level)/d(day). Positive = flooding.
    pub tide_rate: f64,
    pub star_activity: f64,
    pub flare: f64,
    pub supernova: f64,
}

#[derive(Clone, Debug)]
pub struct Cosmos {
    pub p: CosmosParams,
}

fn hash64(mut x: u64) -> u64 {
    x ^= x >> 33;
    x = x.wrapping_mul(0xff51_afd7_ed55_8ccd);
    x ^= x >> 33;
    x = x.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
    x ^ (x >> 33)
}

fn unit(h: u64) -> f64 {
    (h >> 11) as f64 / (1u64 << 53) as f64
}

impl Cosmos {
    pub fn new(p: CosmosParams) -> Self {
        Self { p }
    }

    /// Value noise in 0..1, smooth over `scale` days. Weather without state.
    fn noise(&self, t: f64, scale: f64, salt: u64) -> f64 {
        let x = t / scale;
        let i = x.floor();
        let f = x - i;
        let a = unit(hash64(self.p.seed ^ salt ^ (i as i64 as u64)));
        let b = unit(hash64(self.p.seed ^ salt ^ ((i as i64 + 1) as u64)));
        let s = f * f * (3.0 - 2.0 * f);
        a + (b - a) * s
    }

    /// Sum of event pulses: each day-window may hold one event at a hashed
    /// time with a hashed strength; pulse decays with `tau` days. Scans back
    /// `lookback` windows so a long tail still counts.
    fn events(&self, t: f64, window: f64, prob: f64, tau: f64, lookback: i64, salt: u64) -> f64 {
        let w = (t / window).floor() as i64;
        let mut sum = 0.0;
        for k in (w - lookback)..=w {
            let h = hash64(self.p.seed ^ salt ^ (k as u64).wrapping_mul(0x9E37_79B9));
            if unit(h) >= prob {
                continue;
            }
            let at = (k as f64 + unit(hash64(h))) * window;
            let dt = t - at;
            if dt >= 0.0 {
                let strength = 0.3 + 1.7 * unit(hash64(h ^ 0xABCD));
                sum += strength * (-dt / tau).exp();
            }
        }
        sum
    }

    pub fn sample(&self, t: f64) -> EnvSample {
        let p = &self.p;
        let year = t / p.year_days;
        let year_phase = year.fract();

        // Kepler-lite: mean anomaly -> distance; good enough for e < 0.2
        let m = TAU * year_phase;
        let ecc_anom = m + p.eccentricity * m.sin();
        let orbit_distance = 1.0 - p.eccentricity * ecc_anom.cos();

        let day_phase = t.fract();
        let declination = p.obliquity * (TAU * year_phase).sin();
        let hour_angle = TAU * (day_phase - 0.5);
        let sun_height = p.latitude.sin() * declination.sin() + p.latitude.cos() * declination.cos() * hour_angle.cos();

        let star_activity = 0.5 + 0.5 * (TAU * year / p.star_cycle_years).sin() + 0.15 * (self.noise(t, 30.0, 7) - 0.5);
        let flare_prob = (p.flare_rate_max * star_activity.clamp(0.05, 1.2)).min(1.0);
        let flare = self.events(t, 1.0, flare_prob, 0.04, 1, 0xF1A2);

        let cloud = (self.noise(t, 0.7, 11) * 0.6 + self.noise(t, 4.0, 13) * 0.4).powf(1.5);
        let irradiance = 1.0 / (orbit_distance * orbit_distance);
        let sun = sun_height.max(0.0);
        let visible = irradiance * sun * (1.0 - 0.75 * cloud);
        // UV: thinner air path near zenith, flares spike it, clouds cut less of it
        let uv = irradiance * sun.powf(1.6) * (1.0 - 0.4 * cloud) * (0.8 + 0.4 * star_activity) * (1.0 + 18.0 * flare);

        let pulsar = {
            let ph = (t / p.pulsar_period_days).fract();
            (-((ph - 0.5) * 40.0).powi(2)).exp()
        };
        let binary = 0.5 + 0.5 * (TAU * t / p.binary_period_days).cos();
        let radio = 0.4 + 0.3 * pulsar + 0.2 * binary + 0.6 * flare;

        let supernova = self.events(t, 100.0, p.supernova_rate * 100.0, 25.0, 6, 0x5A5A);
        let galactic = 0.75 + 0.25 * (TAU * t / p.galactic_period_days).sin();
        // stellar wind deflects galactic rays: anticorrelated with activity
        let cosmic = galactic * (1.15 - 0.3 * star_activity) * (1.0 + 40.0 * supernova) + 3.0 * flare;

        // Temperature: seasonal mean plus a lagged diurnal swing damped by cloud.
        let noon_height_season = (p.latitude - declination).cos();
        let seasonal = 268.0 + 42.0 * noon_height_season * irradiance;
        let diurnal = 9.0 * (1.0 - 0.6 * cloud) * (TAU * (day_phase - 0.62)).cos();
        let temperature_k = seasonal + diurnal + 4.0 * (self.noise(t, 6.0, 17) - 0.5);

        // Tides: equilibrium tide is semidiurnal in each body's hour angle.
        let rot = TAU * t;
        let sun_lon = TAU * year_phase;
        let mut tide = p.tide_amps[2] * (2.0 * (rot - sun_lon)).cos();
        let mut tide_rate = -p.tide_amps[2] * 2.0 * TAU * (1.0 - 1.0 / p.year_days) * (2.0 * (rot - sun_lon)).sin();
        for (i, per) in p.moon_periods.iter().enumerate() {
            let lon = TAU * t / per;
            let w = TAU * (1.0 - 1.0 / per);
            tide += p.tide_amps[i] * (2.0 * (rot - lon)).cos();
            tide_rate += -p.tide_amps[i] * 2.0 * w * (2.0 * (rot - lon)).sin();
        }
        // closer moon at perigee is not modelled; storms add surge
        let surge = 0.05 * (self.noise(t, 2.5, 19) - 0.5);

        EnvSample {
            t_days: t,
            day_phase,
            year_phase,
            year,
            sun_height,
            orbit_distance,
            cloud,
            visible,
            uv,
            radio,
            cosmic,
            temperature_k,
            kt: temperature_k / 300.0,
            tide_level: (p.mean_sea_level + tide + surge).clamp(0.0, 1.0),
            tide_rate,
            star_activity,
            flare,
            supernova,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn day_night_and_seasons_vary() {
        let c = Cosmos::new(CosmosParams::default());
        let noon = c.sample(10.5);
        let midnight = c.sample(10.0);
        assert!(noon.visible > 0.1 && midnight.visible == 0.0);
        let summer = c.sample(c.p.year_days * 0.25 + 0.5);
        let winter = c.sample(c.p.year_days * 0.75 + 0.5);
        assert!(summer.temperature_k > winter.temperature_k + 10.0);
    }

    #[test]
    fn tides_have_spring_and_neap() {
        let c = Cosmos::new(CosmosParams::default());
        let mut ranges = Vec::new();
        for d in 0..40 {
            let (mut lo, mut hi) = (1.0f64, 0.0f64);
            for k in 0..96 {
                let s = c.sample(d as f64 + k as f64 / 96.0);
                lo = lo.min(s.tide_level);
                hi = hi.max(s.tide_level);
            }
            ranges.push(hi - lo);
        }
        let max = ranges.iter().cloned().fold(0.0, f64::max);
        let min = ranges.iter().cloned().fold(1.0, f64::min);
        assert!(max > 1.5 * min, "spring/neap contrast: {min}..{max}");
    }

    #[test]
    fn deterministic() {
        let a = Cosmos::new(CosmosParams::default()).sample(123.456);
        let b = Cosmos::new(CosmosParams::default()).sample(123.456);
        assert_eq!(a.uv, b.uv);
        assert_eq!(a.cosmic, b.cosmic);
    }
}

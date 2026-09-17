use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SummaryStats {
    count: u64,
    sum: f64,
    min: f64,
    max: f64,
    mean: f64,
    m2: f64,
}

impl Default for SummaryStats {
    fn default() -> Self {
        Self::new()
    }
}

impl SummaryStats {
    pub fn new() -> Self {
        Self {
            count: 0,
            sum: 0.0,
            min: f64::MAX,
            max: f64::MIN,
            mean: 0.0,
            m2: 0.0,
        }
    }

    pub fn add_value(&mut self, value: f64) {
        self.count += 1;
        self.sum += value;
        if value < self.min {
            self.min = value;
        }
        if value > self.max {
            self.max = value;
        }

        // Welford's algorithm for online mean and variance
        let delta = value - self.mean;
        self.mean += delta / (self.count as f64);
        let delta2 = value - self.mean;
        self.m2 += delta * delta2;
    }

    #[inline]
    pub fn count(&self) -> u64 {
        self.count
    }

    #[inline]
    pub fn sum(&self) -> f64 {
        if self.count == 0 {
            0.0
        } else {
            self.sum
        }
    }

    #[inline]
    pub fn mean(&self) -> f64 {
        if self.count == 0 {
            0.0
        } else {
            self.mean
        }
    }

    #[inline]
    pub fn min(&self) -> f64 {
        if self.count == 0 {
            0.0
        } else {
            self.min
        }
    }

    #[inline]
    pub fn max(&self) -> f64 {
        if self.count == 0 {
            0.0
        } else {
            self.max
        }
    }

    #[inline]
    pub fn variance(&self) -> f64 {
        if self.count < 2 {
            0.0
        } else {
            let var = self.m2 / ((self.count - 1) as f64);
            if var.is_nan() || var < 0.0 {
                0.0
            } else {
                var
            }
        }
    }

    #[inline]
    pub fn std_dev(&self) -> f64 {
        if self.count < 2 {
            0.0
        } else {
            self.variance().sqrt()
        }
    }

    pub fn clear(&mut self) {
        self.count = 0;
        self.sum = 0.0;
        self.min = f64::MAX;
        self.max = f64::MIN;
        self.mean = 0.0;
        self.m2 = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_summary_stats() {
        let mut stats = SummaryStats::new();
        assert_eq!(stats.count(), 0);
        assert_eq!(stats.mean(), 0.0);
        assert_eq!(stats.std_dev(), 0.0);

        stats.add_value(10.0);
        assert_eq!(stats.count(), 1);
        assert_eq!(stats.sum(), 10.0);
        assert_eq!(stats.mean(), 10.0);
        assert_eq!(stats.min(), 10.0);
        assert_eq!(stats.max(), 10.0);
        assert_eq!(stats.variance(), 0.0);
        assert_eq!(stats.std_dev(), 0.0);

        stats.add_value(20.0);
        assert_eq!(stats.count(), 2);
        assert_eq!(stats.sum(), 30.0);
        assert_eq!(stats.mean(), 15.0);
        assert_eq!(stats.min(), 10.0);
        assert_eq!(stats.max(), 20.0);
        assert_eq!(stats.variance(), 50.0);
        assert!((stats.std_dev() - 7.0710678118654755).abs() < 1e-9);
    }
}

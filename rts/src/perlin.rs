use std::num::Wrapping;

// source: https://github.com/gp-97/perlin2d

/// Perlin noise generator parameters:
///
/// * `octaves` - The amount of detail in Perlin noise.
/// * `amplitude` - The maximum absolute value that the Perlin noise can output.
/// * `frequeny` - The number of cycles per unit length that the Perlin noise outputs.
/// * `persistence` - A multiplier that determines how quickly the amplitudes diminish for each successive octave in a Perlin-noise function.
/// * `lacunarity` - A multiplier that determines how quickly the frequency increases for each successive octave in a Perlin-noise function.
/// * `scale` - A Tuple. A number that determines at what distance to view the noisemap.
/// * `seed` -  A value that changes the output of a coherent-noise function.
/// * `bias` - Amount of change in Perlin noise. Used , for example, to make all Perlin noise values positive.
///
/// Additional Info:
/// http://libnoise.sourceforge.net/glossary/
#[derive(Clone, Copy)]
pub struct PerlinNoise2D {
    pub octaves: i32,
    pub amplitude: f64,
    pub frequency: f64,
    pub persistence: f64,
    pub lacunarity: f64,
    pub scale: (f64, f64),
    pub bias: f64,
    pub seed: i32,
}

impl PerlinNoise2D {
    /// generates and returns 2D perlin noise
    pub fn get_noise(&self, x: f64, y: f64) -> f64 {
        self.bias + self.amplitude * self.total(x / self.scale.0, y / self.scale.1)
    }

    fn total(&self, x: f64, y: f64) -> f64 {
        let mut t = 0.0;
        let mut amp = 1.0;
        let mut freq = self.frequency;

        for _ in 0..self.octaves {
            t += self.get_value(y * freq + self.seed as f64, x * freq + self.seed as f64) * amp;
            amp *= self.persistence;
            freq *= self.lacunarity;
        }
        t
    }

    fn interpolate(&self, x: f64, y: f64, a: f64) -> f64 {
        let neg_a: f64 = 1.0 - a;
        let neg_a_sqr: f64 = neg_a * neg_a;
        let fac1: f64 = 3.0 * (neg_a_sqr) - 2.0 * (neg_a_sqr * neg_a);
        let a_sqr: f64 = a * a;
        let fac2: f64 = 3.0 * a_sqr - 2.0 * (a_sqr * a);

        x * fac1 + y * fac2 // add the weighted factors
    }

    fn noise(&self, x: i32, y: i32) -> f64 {
        let mut n: i64 = x as i64 + y as i64 * 57;
        n = (n << 13) ^ n;
        let t = Wrapping(n) * Wrapping(n) * Wrapping(n * 15731 + 789221) + Wrapping(1376312589);
        let t = t.0 & 0x7fffffff;
        1.0 - (t as f64) * 0.931322574615478515625e-9
    }

    fn get_value(&self, x: f64, y: f64) -> f64 {
        let x_int: i32 = x as i32;
        let y_int: i32 = y as i32;
        let x_frac: f64 = x - f64::floor(x);
        let y_frac: f64 = y - f64::floor(y);

        // noise values
        let n01: f64 = self.noise(x_int - 1, y_int - 1);
        let n02: f64 = self.noise(x_int + 1, y_int - 1);
        let n03: f64 = self.noise(x_int - 1, y_int + 1);
        let n04: f64 = self.noise(x_int + 1, y_int + 1);
        let n05: f64 = self.noise(x_int - 1, y_int);
        let n06: f64 = self.noise(x_int + 1, y_int);
        let n07: f64 = self.noise(x_int, y_int - 1);
        let n08: f64 = self.noise(x_int, y_int + 1);
        let n09: f64 = self.noise(x_int, y_int);

        let n12: f64 = self.noise(x_int + 2, y_int - 1);
        let n14: f64 = self.noise(x_int + 2, y_int + 1);
        let n16: f64 = self.noise(x_int + 2, y_int);

        let n23: f64 = self.noise(x_int - 1, y_int + 2);
        let n24: f64 = self.noise(x_int + 1, y_int + 2);
        let n28: f64 = self.noise(x_int, y_int + 2);

        let n34: f64 = self.noise(x_int + 2, y_int + 2);

        // find the noise values of the four corners
        let x0y0: f64 =
            0.0625 * (n01 + n02 + n03 + n04) + 0.125 * (n05 + n06 + n07 + n08) + 0.25 * (n09);
        let x1y0: f64 =
            0.0625 * (n07 + n12 + n08 + n14) + 0.125 * (n09 + n16 + n02 + n04) + 0.25 * (n06);
        let x0y1: f64 =
            0.0625 * (n05 + n06 + n23 + n24) + 0.125 * (n03 + n04 + n09 + n28) + 0.25 * (n08);
        let x1y1: f64 =
            0.0625 * (n09 + n16 + n28 + n34) + 0.125 * (n08 + n14 + n06 + n24) + 0.25 * (n04);

        // interpolate between those values according to the x and y fractions
        let v1: f64 = self.interpolate(x0y0, x1y0, x_frac); // interpolate in x
                                                            // direction (y)
        let v2: f64 = self.interpolate(x0y1, x1y1, x_frac); // interpolate in x
                                                            // direction (y+1)
        let fin: f64 = self.interpolate(v1, v2, y_frac); // interpolate in y direction

        return fin;
    }
}

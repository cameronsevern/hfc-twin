//! The spectrum CNN as a data file, evaluated in-process: the same network as `ml/train_spectrum.py`. It reads one modem's
//! RxMER curve and names shapes, bands and falls. A shape isn't a cause. See `docs/tasks/17-spectrum-model.md`.

use features::spectrum::{BIN_MHZ, BINS, Binned, PADDING};
use schema::spectrum::Shape;
use serde::{Deserialize, Serialize};

use crate::model::ModelError;

/// The channels the network reads: lowest, mean, frequency, present.
const INPUTS: usize = 4;
/// The top of the frequency channel's scale, MHz.
const TOP_MHZ: f64 = 1218.0;
/// The network this code evaluates.
const ARCHITECTURE: &str = "dilated-cnn-2";
/// The scale of the two curves, dB.
const CURVE_SCALE_DB: f64 = 50.0;
const STEPS_PER_DB: f64 = 4.0;

/// One 1-D convolution, zero-padded so its output is as long as its input.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Conv {
    /// Taps.
    pub kernel: usize,
    /// Spacing of the taps, bins.
    pub dilation: usize,
    /// Weights, ordered output, input, tap.
    pub w: Vec<f64>,
    /// Bias, per output.
    pub b: Vec<f64>,
    /// Input channels.
    pub inputs: usize,
    /// Output channels.
    pub outputs: usize,
}

impl Conv {
    fn check(&self, name: &str) -> Result<(), ModelError> {
        if self.w.len() != self.outputs * self.inputs * self.kernel || self.b.len() != self.outputs
        {
            return Err(ModelError::Shape(format!("{name} has the wrong shape")));
        }
        Ok(())
    }

    /// `x` is `inputs` channels of `BINS`, end to end.
    fn apply(&self, x: &[f64]) -> Vec<f64> {
        let mut out = vec![0.0; self.outputs * BINS];
        let half = (self.kernel / 2) as isize * self.dilation as isize;
        for o in 0..self.outputs {
            let row = &mut out[o * BINS..(o + 1) * BINS];
            row.fill(self.b[o]);
            for i in 0..self.inputs {
                let channel = &x[i * BINS..(i + 1) * BINS];
                for k in 0..self.kernel {
                    let weight = self.w[(o * self.inputs + i) * self.kernel + k];
                    let shift = k as isize * self.dilation as isize - half;
                    if shift >= 0 {
                        let shift = shift as usize;
                        for (a, b) in row[..BINS - shift].iter_mut().zip(&channel[shift..]) {
                            *a += weight * b;
                        }
                    } else {
                        let shift = (-shift) as usize;
                        for (a, b) in row[shift..].iter_mut().zip(&channel[..BINS - shift]) {
                            *a += weight * b;
                        }
                    }
                }
            }
        }
        out
    }
}

/// How the per-bin outputs become an answer, chosen on the `calibrate` maps.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Thresholds {
    /// Per shape: the probability that marks a bin.
    pub probability: Vec<f64>,
    /// Per shape: how many marked bins make it present.
    pub bins: Vec<usize>,
    /// A fall this large, dB, with no shape, is "don't know".
    pub unknown_fall_db: f64,
    /// Over this many bins.
    pub unknown_bins: usize,
}

/// The model file.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct SpectrumModel {
    /// Its name.
    pub name: String,
    /// The fixed network this holds weights for.
    pub architecture: String,
    /// Whether it trained on simulated or real data.
    pub origin: String,
    /// The observation schema it trained on.
    pub observation_schema: String,
    /// The fault schema it trained on.
    pub fault_schema: String,
    /// The label schema it trained on.
    pub spectrum_schema: String,
    /// Shape names, in output order.
    pub shapes: Vec<String>,
    /// First convolution.
    pub stem: Conv,
    /// Residual blocks.
    pub blocks: Vec<Conv>,
    /// Joins each bin to the summary of the whole channel.
    pub mix: Conv,
    /// Per-bin outputs: one logit per shape, then the fall in dB.
    pub head: Conv,
    /// How to read the outputs, once chosen.
    #[serde(default)]
    pub thresholds: Option<Thresholds>,
}

/// One shape found in a capture.
#[derive(Debug, Clone, PartialEq)]
pub struct FoundShape {
    /// The shape.
    pub shape: Shape,
    /// Lowest marked frequency, MHz.
    pub lo_mhz: f64,
    /// Highest marked frequency, MHz.
    pub hi_mhz: f64,
    /// The largest predicted fall among its bins, dB.
    pub worst_fall_db: f64,
}

/// What the model says about one capture.
#[derive(Debug, Clone, PartialEq)]
pub struct Reading {
    /// Shapes it can name.
    pub shapes: Vec<FoundShape>,
    /// Something fell and it can't say what.
    pub unknown: bool,
    /// The largest predicted fall, dB.
    pub worst_fall_db: f64,
}

impl Reading {
    /// Whether it calls this capture healthy.
    pub fn normal(&self) -> bool {
        self.shapes.is_empty() && !self.unknown
    }
}

/// The network's input from a capture's bytes, end to end. Quarter-dB bytes, as training saw.
pub fn input_channels(first_mhz: f64, low: &[u8], mean: &[u8]) -> Vec<f64> {
    let present: Vec<bool> = mean.iter().map(|b| *b != PADDING).collect();
    let count = present.iter().filter(|p| **p).count().max(1);
    let total: f64 = mean
        .iter()
        .zip(&present)
        .filter(|(_, p)| **p)
        .map(|(b, _)| f64::from(*b))
        .sum();
    let overall = total / count as f64;
    let scale = STEPS_PER_DB * CURVE_SCALE_DB;
    let mut out = Vec::with_capacity(INPUTS * BINS);
    let curve = |values: &[u8], out: &mut Vec<f64>| {
        for (b, p) in values.iter().zip(&present) {
            out.push(if *p { f64::from(*b) } else { overall } / scale);
        }
    };
    curve(low, &mut out);
    curve(mean, &mut out);
    out.extend((0..BINS).map(|k| (first_mhz + k as f64 * BIN_MHZ) / TOP_MHZ));
    out.extend(present.iter().map(|p| f64::from(u8::from(*p))));
    out
}

impl SpectrumModel {
    /// Refuses a file from another schema or the wrong shape (invariant 3).
    pub fn check(&self) -> Result<(), ModelError> {
        if self.architecture != ARCHITECTURE {
            return Err(ModelError::Shape(format!(
                "holds weights for {}, this runs {ARCHITECTURE}",
                self.architecture
            )));
        }
        // A model trained on an earlier observation version still reads this one's (task 22).
        if !schema::observation::model_can_read(&self.observation_schema) {
            return Err(ModelError::ObservationSchema {
                found: self.observation_schema.clone(),
                expected: schema::observation::SCHEMA_VERSION.to_string(),
            });
        }
        if !schema::fault::model_can_read(&self.fault_schema) {
            return Err(ModelError::FaultSchema {
                found: self.fault_schema.clone(),
                expected: schema::fault::SCHEMA_VERSION.to_string(),
            });
        }
        if !schema::spectrum::model_can_read(&self.spectrum_schema) {
            return Err(ModelError::Shape(format!(
                "trained on spectrum schema {}, this is {}",
                self.spectrum_schema,
                schema::spectrum::SCHEMA_VERSION
            )));
        }
        if self.shapes.len() != Shape::ALL.len() || self.head.outputs != Shape::ALL.len() + 1 {
            return Err(ModelError::Shape("wrong outputs".into()));
        }
        self.stem.check("stem")?;
        self.mix.check("mix")?;
        if self.mix.inputs != 2 * self.mix.outputs {
            return Err(ModelError::Shape("mix has the wrong shape".into()));
        }
        self.head.check("head")?;
        for block in &self.blocks {
            block.check("block")?;
        }
        Ok(())
    }

    /// The per-bin outputs, end to end: one logit per shape, then the fall.
    pub fn forward(&self, input: &[f64]) -> Vec<f64> {
        let relu = |v: &mut [f64]| v.iter_mut().for_each(|x| *x = x.max(0.0));
        let mut h = self.stem.apply(input);
        relu(&mut h);
        for block in &self.blocks {
            let mut step = block.apply(&h);
            relu(&mut step);
            h.iter_mut().zip(&step).for_each(|(a, b)| *a += b);
        }
        // A flat fall has no edge for a convolution to find, so every bin also sees the mean of each channel, padding left out.
        let present = &input[(INPUTS - 1) * BINS..INPUTS * BINS];
        let count = present.iter().sum::<f64>().max(1.0);
        let mut joined = h.clone();
        for channel in h.as_chunks::<BINS>().0 {
            let total: f64 = channel.iter().zip(present).map(|(a, p)| a * p).sum();
            joined.resize(joined.len() + BINS, total / count);
        }
        let mut step = self.mix.apply(&joined);
        relu(&mut step);
        h.iter_mut().zip(&step).for_each(|(a, b)| *a += b);
        self.head.apply(&h)
    }

    /// Reads a binned capture. `None` until thresholds are chosen.
    pub fn read(&self, binned: &Binned) -> Option<Reading> {
        let thresholds = self.thresholds.as_ref()?;
        let (low, mean) = binned.to_bytes();
        let input = input_channels(binned.first_mhz, &low, &mean);
        let out = self.forward(&input);
        let channel = |c: usize| &out[c * BINS..(c + 1) * BINS];
        let fall = channel(Shape::ALL.len());
        let at_mhz = |k: usize| binned.first_mhz + k as f64 * BIN_MHZ;
        let mut shapes = Vec::new();
        let mut named = vec![false; BINS];
        for (s, shape) in Shape::ALL.iter().enumerate() {
            let logits = channel(s);
            let cut = thresholds.probability[s];
            let marked: Vec<usize> = (0..BINS)
                .filter(|k| binned.present[*k] && sigmoid(logits[*k]) >= cut)
                .collect();
            if marked.len() >= thresholds.bins[s] && !marked.is_empty() {
                marked.iter().for_each(|k| named[*k] = true);
                shapes.push(FoundShape {
                    shape: *shape,
                    lo_mhz: at_mhz(marked[0]),
                    hi_mhz: at_mhz(marked[marked.len() - 1]),
                    worst_fall_db: marked.iter().map(|k| fall[*k]).fold(0.0, f64::max),
                });
            }
        }
        let falling = (0..BINS)
            .filter(|k| binned.present[*k] && !named[*k] && fall[*k] >= thresholds.unknown_fall_db)
            .count();
        let worst_fall_db = (0..BINS)
            .filter(|k| binned.present[*k])
            .map(|k| fall[k])
            .fold(0.0, f64::max);
        Some(Reading {
            shapes,
            unknown: falling >= thresholds.unknown_bins,
            worst_fall_db,
        })
    }
}

fn sigmoid(x: f64) -> f64 {
    1.0 / (1.0 + (-x).exp())
}

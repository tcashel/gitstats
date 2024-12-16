# Benchmark Results

This directory contains benchmark baselines and historical results for GitStats.

## Directory Structure

- `baseline/` - Current baseline for comparison
- `baselines/` - Historical baseline snapshots (YYYYMMDD format)

## Initial Setup

```bash
# Run benchmarks to generate initial data
cargo bench

# Create directory structure
mkdir -p benchmarks/baselines
mkdir -p benchmarks/baseline

# Save initial baseline
cp -r target/criterion/* benchmarks/baselines/$(date +%Y%m%d)
cp -r target/criterion/* benchmarks/baseline
```

## Saving New Baselines

```bash
# Run the benchmarks
cargo bench

# Save as new baseline
cp -r target/criterion/* benchmarks/baselines/$(date +%Y%m%d)
cp -r target/criterion/* benchmarks/baseline
```

## Interpreting Results

- Check HTML reports in `target/criterion/report/index.html`
- Look for statistical significance in changes
- Review performance regression warnings
- Compare against historical baselines for trends

## Notes

- Baselines are stored in version control
- CI uses these baselines for regression testing
- Historical baselines help track performance over time
- The `target/criterion/` directory contains the latest run results
- Baselines in this directory are used for comparison
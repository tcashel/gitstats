# Benchmark Results

This directory contains benchmark baselines and historical results for GitStats.

## Directory Structure

- `baseline/` - Current baseline for comparison
- `baselines/` - Historical baseline snapshots (YYYYMMDD format)

## Running Benchmarks

```bash
# Run benchmarks against current baseline
cargo bench -- --baseline benchmarks/baseline

# Run benchmarks against specific historical baseline
cargo bench -- --baseline benchmarks/baselines/20231201
```

## Saving New Baselines

```bash
# Save current results as new baseline
mkdir -p benchmarks/baselines
cp -r target/criterion/baseline benchmarks/baselines/$(date +%Y%m%d)
cp -r target/criterion/baseline benchmarks/baseline
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
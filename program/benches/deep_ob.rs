#[cfg(feature = "benchmarking")]
use bonfida_utils::{bench::BenchRunner, test_name};

#[cfg(feature = "benchmarking")]
pub fn main() {
    let samples = 10;
    let max_order_capacity = 100_000;
    let bench_runner = BenchRunner::new(test_name!(), agnostic_orderbook::ID);

    let order_capacities = (1..max_order_capacity)
        .step_by((max_order_capacity / samples) as usize)
        .collect::<Vec<_>>();

    let mut compute_budget = Vec::with_capacity(99);

    for order_capacity in order_capacities.iter() {
        let res = bench_runner.run(&[order_capacity.to_string()]);
        compute_budget.push(res[0]);
    }
    bench_runner.commit(order_capacities, compute_budget);
}

#[cfg(not(feature = "benchmarking"))]
pub fn main() {}

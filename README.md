# Asset agnostic orderbook

Orderbook program which can be used with generic assets.

## Overview

This program is intended to be called upon by other programs that implement specific on-chain orderbooks.
These "caller" program can use the agnostic orderbook as an underlying infrastructure.

## Documentation

Run `cargo doc --open` in the `program` directory to open detailed API documentation.

## FAQ

### What does FP32 mean and how does it work?

FP32 stands for _Fixed Point at 32 bit number_. Floating point numbers are too computationally expensive to manipulate in the Solana runtime. The idea behind FP32 is to store floats as integers with a fixed exponent. For instance, if we want to express the number `2.5` as an FP1 number, we use the integer `5` because `5 = floor(2.5 * (2 ^ 1))`. FP32 numbers work in the same way : if we want to express a number `n`, we use the integer `floor(n * (2 ** 32))` instead.

We use powers of two as constants because multiplications and divisions by powers of two are extremely easy and efficient in binary representation, for the same reason that multiplications and divisions by 10 are easy in decimal representation. The operation used to multiply by a power of two is called a _bitshift_, because that's exactly what is happening : bits are shifted to the right to divide by 2, and to the left to multiply by 2 :

- Multiplication by 2 : `n * (2**k) = n << k`
- Integer division by 2 : `n / (2**k) = n >> k`

Let's take a look at how basic math operations work on FP32 number :

| Operation      | Int     | FP32            |
| -------------- | ------- | --------------- |
| Addition       | `a + b` | `a + b`         |
| Substraction   | `a - b` | `a - b`         |
| Multiplication | `a * b` | `(a * b) >> 32` |
| Division       | `a / b` | `(a << 32) / b` |

In practice, the multiplication and division formulae have one issue : the `a * b` and `a << 32` operations have a high chance of _overflowing_ their `u64` integer representations. To eliminate this problem, we always cast `a` and `b` to an integer type with a higher bitdepth :

- Multiplication : `(((a as u128) * (b as u128)) >> 32) as u64`
- Division : `(((a as u128) << 32) / (b as u128)) as u64`

Utility function for FP32 operations can be found [here](https://github.com/Bonfida/bonfida-utils/blob/main/utils/src/fp_math.rs).

# VL53L5CX driver

Vendored without modification from pollen-robotics/microduck commit
942e58e3aa6dbb28e7037203fff11866122c7567, tof/vendor/platform.c and
 tof/vendor/vl53l5cx/. ST ULD and firmware: BSD-3-Clause (LICENSE.txt and
source notices); MicroDuck platform/shim: Apache-2.0 (APACHE-2.0.txt).
Source: https://github.com/pollen-robotics/microduck/tree/942e58e3aa6dbb28e7037203fff11866122c7567/tof/vendor

Observer uses only VL53L5CX over I2C, fixed address 0x29, 8x8 at 15 Hz.
No old tofd service or automatic bus probing is used.

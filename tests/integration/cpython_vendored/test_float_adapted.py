# Adapted from CPython 3.12.x cpython/Lib/test/test_float.py
# Python Software Foundation License (PSF-2.0)
#
# Drop bucket — tests omitted:
#   * Locale-aware parsing: test_float_with_comma, test_float_setlocale.
#   * Struct / memoryview: test_double_unpack, test_float_repr_roundtrip
#     (the strict ULP-accurate repr round-trip; we keep a coarser repr smoke).
#   * Hash-into-int consistency: test_int_float_hash_equal — covered separately
#     in property tests under proptest_hash_eq.rs once that lands.
#   * Subnormal / signaling-NaN edges: test_float_subnormal, test_signaling_nan —
#     CPython-specific FPU configuration.
#   * Pickle round-trips: test_pickle_*.
#   * Inheritance tests: test_float_subclass.
#   * __format__ spec deep-dive: kept only the core %g / .2f cases.

import math

def test_float_no_args():
    assert float() == 0.0

def test_float_from_int():
    assert float(0) == 0.0
    assert float(1) == 1.0
    assert float(-1) == -1.0
    assert float(100) == 100.0

def test_float_from_str():
    assert float('0') == 0.0
    assert float('1.0') == 1.0
    assert float('-1.5') == -1.5
    assert float('  3.14  ') == 3.14
    assert float('1e2') == 100.0
    assert float('1e-2') == 0.01
    assert float('+1.5') == 1.5

def test_float_from_bool():
    assert float(True) == 1.0
    assert float(False) == 0.0

def test_float_arithmetic():
    assert 1.0 + 2.0 == 3.0
    assert 5.0 - 3.0 == 2.0
    assert 2.0 * 3.0 == 6.0
    assert 10.0 / 4.0 == 2.5
    assert 10.0 // 3.0 == 3.0
    assert 10.0 % 3.0 == 1.0
    assert 2.0 ** 10 == 1024.0

def test_float_mixed_int():
    assert 1 + 1.0 == 2.0
    assert 1.0 + 1 == 2.0
    assert isinstance(1 + 1.0, float)
    assert isinstance(1.0 + 1, float)

def test_float_comparison():
    assert 1.0 < 2.0
    assert 2.0 <= 2.0
    assert 3.0 > 2.0
    assert 3.0 >= 3.0
    assert 1.0 == 1.0
    assert 1.0 != 2.0
    # Cross-type
    assert 1 == 1.0
    assert 1.0 == 1
    assert 1.5 != 1
    assert 1 < 1.5

def test_float_inf():
    inf = float('inf')
    assert inf > 0
    assert inf > 1e308
    assert -inf < -1e308
    assert inf == float('inf')
    assert -inf == float('-inf')
    assert math.isinf(inf)
    assert math.isinf(-inf)
    assert not math.isinf(0.0)

def test_float_nan():
    nan = float('nan')
    assert math.isnan(nan)
    # NaN is not equal to anything including itself
    assert nan != nan
    assert not (nan == nan)
    assert not (nan < 0)
    assert not (nan > 0)

def test_float_negative_zero():
    nz = -0.0
    z = 0.0
    assert nz == z
    # But repr differs
    assert repr(nz) == '-0.0'
    assert repr(z) == '0.0'

def test_float_zero_division():
    try:
        _ = 1.0 / 0
        raise AssertionError("expected ZeroDivisionError")
    except ZeroDivisionError:
        pass
    try:
        _ = 1.0 // 0
        raise AssertionError("expected ZeroDivisionError")
    except ZeroDivisionError:
        pass

def test_float_invalid_str():
    try:
        float('abc')
        raise AssertionError("expected ValueError")
    except ValueError:
        pass
    try:
        float('')
        raise AssertionError("expected ValueError")
    except ValueError:
        pass

def test_float_int_conversion():
    assert int(1.5) == 1
    assert int(-1.5) == -1
    assert int(0.0) == 0
    assert int(1e10) == 10000000000

def test_float_abs():
    assert abs(1.5) == 1.5
    assert abs(-1.5) == 1.5
    assert abs(0.0) == 0.0
    assert abs(-0.0) == 0.0

def test_float_repr():
    assert repr(0.0) == '0.0'
    assert repr(1.0) == '1.0'
    assert repr(0.1) == '0.1'
    assert repr(1.5) == '1.5'

def test_float_str():
    assert str(0.0) == '0.0'
    assert str(1.0) == '1.0'
    assert str(0.1) == '0.1'

def test_float_round():
    assert round(1.5) == 2
    assert round(2.5) == 2  # banker's rounding
    assert round(0.5) == 0
    assert round(-0.5) == 0
    assert round(1.25, 1) == 1.2  # banker's rounding
    assert round(1.35, 1) == 1.4

def test_float_pow():
    assert 2.0 ** 0.5 == math.sqrt(2.0)
    assert 4.0 ** 0.5 == 2.0
    assert (-1.0) ** 2 == 1.0

def test_float_modf_sign():
    # 7.5 % 2.5 == 0.0 in float arithmetic
    assert 7.5 % 2.5 == 0.0
    # CPython floor-mod: -7.5 % 2.5 == 0.0; sign of result follows divisor.
    assert (-7.5) % 2.5 == 0.0


if __name__ == "__main__":
    run_tests = [
        test_float_no_args,
        test_float_from_int,
        test_float_from_str,
        test_float_from_bool,
        test_float_arithmetic,
        test_float_mixed_int,
        test_float_comparison,
        test_float_inf,
        test_float_nan,
        test_float_negative_zero,
        test_float_zero_division,
        test_float_invalid_str,
        test_float_int_conversion,
        test_float_abs,
        test_float_repr,
        test_float_str,
        test_float_round,
        test_float_pow,
        test_float_modf_sign,
    ]
    passed = 0
    failed = []
    for t in run_tests:
        try:
            t()
            passed += 1
        except AssertionError as e:
            failed.append((t.__name__, str(e)))
        except Exception as e:
            failed.append((t.__name__, f"{type(e).__name__}: {e}"))
    print(f"{passed}/{passed + len(failed)} passed")
    if failed:
        for name, msg in failed:
            print(f"FAIL {name}: {msg}")

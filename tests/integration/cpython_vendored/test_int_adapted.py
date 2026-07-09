# Adapted from CPython 3.12.x cpython/Lib/test/test_int.py
# Python Software Foundation License (PSF-2.0)
#
# Drop bucket — tests omitted:
#   * sys-internal int-str digit limits: test_int_max_str_digits_*, test_disabled_limit,
#     test_max_str_digits_edge_cases, test_denial_of_service_* — depend on
#     sys.set_int_max_str_digits which is a CPython runtime knob.
#   * Memoryview / buffer protocol: test_int_memoryview.
#   * Int-subclass dunder dispatch: test_int_subclass_with_index, test_int_subclass_with_int,
#     test_int_returns_int_subclass, test_int_base_indexable — require __index__ dispatch.
#   * Underscore literals in non-trivial positions: test_underscores, test_underscores_ignored,
#     test_sign_not_counted — partially kept the simple cases.
#   * PyLong-specific: test_pylong_*, test_int_from_other_bases — exercise the C
#     long-to-str/str-to-long algorithms directly.
#   * Issue regressions: test_issue31619.
#   * test_basic from CPython is ~180 lines of base-conversion and edge-case
#     sweeps; we kept the high-signal core (decimal, hex, binary, sign handling)
#     and dropped the exhaustive table.

def test_int_no_args():
    assert int() == 0

def test_int_from_int():
    assert int(0) == 0
    assert int(1) == 1
    assert int(-1) == -1
    assert int(100) == 100

def test_int_from_str():
    assert int('0') == 0
    assert int('1') == 1
    assert int('-1') == -1
    assert int('123') == 123
    assert int('  42  ') == 42
    assert int('+5') == 5
    assert int('-5') == -5

def test_int_from_str_with_base():
    assert int('10', 2) == 2
    assert int('ff', 16) == 255
    assert int('0xff', 16) == 255
    assert int('0b10', 2) == 2
    assert int('0o17', 8) == 15
    assert int('17', 8) == 15
    assert int('z', 36) == 35

def test_int_from_float():
    assert int(0.0) == 0
    assert int(1.5) == 1
    assert int(-1.5) == -1
    assert int(2.9) == 2
    assert int(-2.9) == -2

def test_int_from_bool():
    # bool is a subclass of int
    assert int(True) == 1
    assert int(False) == 0
    assert True + True == 2
    assert True + 1 == 2
    assert isinstance(True, int)

def test_int_arithmetic():
    assert 1 + 2 == 3
    assert 5 - 3 == 2
    assert 4 * 3 == 12
    assert 10 // 3 == 3
    assert 10 % 3 == 1
    assert 2 ** 10 == 1024
    assert -7 // 2 == -4  # floor division rounds towards -inf
    assert -7 % 2 == 1

def test_int_truediv():
    # truediv returns float
    assert 10 / 4 == 2.5
    assert 6 / 3 == 2.0
    assert isinstance(6 / 3, float)

def test_int_comparison():
    assert 1 < 2
    assert 2 <= 2
    assert 3 > 2
    assert 3 >= 3
    assert 1 == 1
    assert 1 != 2

def test_int_bit_ops():
    assert 5 & 3 == 1
    assert 5 | 3 == 7
    assert 5 ^ 3 == 6
    assert ~0 == -1
    assert ~5 == -6
    assert 1 << 4 == 16
    assert 16 >> 2 == 4

def test_int_negative():
    assert -0 == 0
    assert -(-1) == 1
    assert abs(-5) == 5
    assert abs(5) == 5

def test_int_pow_three_arg():
    # CPython supports the 3-arg form: pow(a, b, mod) == (a ** b) % mod.
    assert pow(2, 10, 1000) == 24
    assert pow(7, 2, 5) == 4

def test_int_divmod():
    assert divmod(10, 3) == (3, 1)
    assert divmod(-10, 3) == (-4, 2)
    assert divmod(10, -3) == (-4, -2)
    assert divmod(0, 5) == (0, 0)

def test_int_str_conversion():
    assert str(0) == '0'
    assert str(123) == '123'
    assert str(-123) == '-123'

def test_int_hex_oct_bin():
    assert hex(255) == '0xff'
    assert oct(8) == '0o10'
    assert bin(5) == '0b101'
    assert hex(0) == '0x0'

def test_int_zero_division():
    try:
        _ = 1 // 0
        raise AssertionError("expected ZeroDivisionError")
    except ZeroDivisionError:
        pass
    try:
        _ = 1 % 0
        raise AssertionError("expected ZeroDivisionError")
    except ZeroDivisionError:
        pass

def test_int_invalid_str():
    try:
        int('abc')
        raise AssertionError("expected ValueError")
    except ValueError:
        pass
    try:
        int('1.5')
        raise AssertionError("expected ValueError")
    except ValueError:
        pass

def test_int_large_values():
    # CPython has arbitrary-precision ints.
    big = 10 ** 50
    assert big == 100000000000000000000000000000000000000000000000000
    assert big + 1 == big + 1
    assert big * 2 // 2 == big

def test_int_small_ints_identity():
    # CPython interns small ints (-5..256); identity matters for our docs.
    a = 1
    b = 1
    assert a is b

def test_int_repr_matches_str():
    assert repr(42) == '42'
    assert repr(-42) == '-42'


if __name__ == "__main__":
    run_tests = [
        test_int_no_args,
        test_int_from_int,
        test_int_from_str,
        test_int_from_str_with_base,
        test_int_from_float,
        test_int_from_bool,
        test_int_arithmetic,
        test_int_truediv,
        test_int_comparison,
        test_int_bit_ops,
        test_int_negative,
        test_int_pow_three_arg,
        test_int_divmod,
        test_int_str_conversion,
        test_int_hex_oct_bin,
        test_int_zero_division,
        test_int_invalid_str,
        test_int_large_values,
        test_int_small_ints_identity,
        test_int_repr_matches_str,
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

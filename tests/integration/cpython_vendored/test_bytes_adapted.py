# Adapted from CPython 3.12.x cpython/Lib/test/test_bytes.py
# Python Software Foundation License (PSF-2.0)
#
# Drop bucket — tests omitted:
#   * Buffer protocol / memoryview: test_buffer_*, test_bytes_to_long.
#   * Pickle: test_pickle_*.
#   * GC / refcount: test_resize, test_gc.
#   * bytes-subclass dispatch: test_bytes_subclass, test_translate_subclass.
#   * Codec-registry interactions: test_decode_*_errors with custom error handlers.
#   * bytearray-vs-bytes mutation race tests: test_iterator_pickling, test_extended_*.
#   * Format-spec corner cases: test_format_huge_*.

def test_bytes_constructor():
    assert bytes() == b''
    assert bytes(0) == b''
    assert bytes(3) == b'\x00\x00\x00'
    assert bytes([1, 2, 3]) == b'\x01\x02\x03'
    assert bytes(b'abc') == b'abc'
    assert bytes('abc', 'utf-8') == b'abc'

def test_bytes_len():
    assert len(b'') == 0
    assert len(b'abc') == 3
    assert len(b'\x00\x01') == 2

def test_bytes_indexing():
    b = b'abc'
    # bytes indexing returns int (a single byte's value)
    assert b[0] == 97
    assert b[1] == 98
    assert b[-1] == 99
    try:
        _ = b[5]
        raise AssertionError("expected IndexError")
    except IndexError:
        pass

def test_bytes_slicing():
    b = b'hello'
    assert b[1:4] == b'ell'
    assert b[:3] == b'hel'
    assert b[3:] == b'lo'
    assert b[:] == b'hello'
    assert b[::-1] == b'olleh'

def test_bytes_concatenation():
    assert b'abc' + b'def' == b'abcdef'
    assert b'' + b'x' == b'x'

def test_bytes_repetition():
    assert b'ab' * 3 == b'ababab'
    assert b'x' * 0 == b''

def test_bytes_contains():
    assert b'ell' in b'hello'
    assert b'xx' not in b'hello'
    # integer-in-bytes: 97 == ord('a')
    assert 97 in b'abc'
    assert 122 not in b'abc'

def test_bytes_equality():
    assert b'abc' == b'abc'
    assert b'abc' != b'abd'
    assert b'' == b''
    # bytes != str
    assert b'abc' != 'abc'

def test_bytes_comparison():
    assert b'a' < b'b'
    assert b'aa' < b'ab'
    assert b'a' < b'aa'

def test_bytes_decode():
    assert b'hello'.decode('utf-8') == 'hello'
    assert b'hello'.decode() == 'hello'
    assert b'\xc3\xa9'.decode('utf-8') == 'é'

def test_bytes_encode_str_round_trip():
    s = 'hello world'
    assert s.encode('utf-8').decode('utf-8') == s

def test_bytes_hex_literal():
    assert b'\x00' == bytes([0])
    assert b'\xff' == bytes([255])
    assert b'\x41' == b'A'

def test_bytes_join():
    assert b','.join([b'a', b'b', b'c']) == b'a,b,c'
    assert b''.join([b'a', b'b']) == b'ab'
    assert b' '.join([]) == b''

def test_bytes_split():
    assert b'a,b,c'.split(b',') == [b'a', b'b', b'c']
    assert b'a b c'.split() == [b'a', b'b', b'c']
    assert b'a,b,c'.split(b',', 1) == [b'a', b'b,c']

def test_bytes_strip():
    assert b'  abc  '.strip() == b'abc'
    assert b'xxabcyy'.strip(b'xy') == b'abc'

def test_bytes_startswith_endswith():
    assert b'hello'.startswith(b'he')
    assert not b'hello'.startswith(b'lo')
    assert b'hello'.endswith(b'lo')
    assert not b'hello'.endswith(b'he')

def test_bytes_find_index():
    assert b'hello'.find(b'll') == 2
    assert b'hello'.find(b'xx') == -1
    assert b'hello'.index(b'll') == 2
    try:
        b'hello'.index(b'xx')
        raise AssertionError("expected ValueError")
    except ValueError:
        pass

def test_bytes_count():
    assert b'hello'.count(b'l') == 2
    assert b'hello'.count(b'xx') == 0

def test_bytes_replace():
    assert b'hello'.replace(b'l', b'L') == b'heLLo'
    assert b'hello'.replace(b'l', b'L', 1) == b'heLlo'

def test_bytes_upper_lower():
    assert b'abc'.upper() == b'ABC'
    assert b'ABC'.lower() == b'abc'

def test_bytes_repr():
    assert repr(b'abc') == "b'abc'"
    assert repr(b'') == "b''"
    assert repr(b'\x00') == "b'\\x00'"

def test_bytes_iteration():
    # iterating bytes yields ints
    ints = []
    for b in b'abc':
        ints.append(b)
    assert ints == [97, 98, 99]

def test_bytearray_basic():
    ba = bytearray(b'abc')
    assert ba == bytearray(b'abc')
    assert ba == b'abc'
    ba[0] = 65
    assert ba == bytearray(b'Abc')
    ba.append(100)
    assert ba == bytearray(b'Abcd')

def test_bytearray_from_str():
    ba = bytearray('hello', 'utf-8')
    assert ba == bytearray(b'hello')


if __name__ == "__main__":
    run_tests = [
        test_bytes_constructor,
        test_bytes_len,
        test_bytes_indexing,
        test_bytes_slicing,
        test_bytes_concatenation,
        test_bytes_repetition,
        test_bytes_contains,
        test_bytes_equality,
        test_bytes_comparison,
        test_bytes_decode,
        test_bytes_encode_str_round_trip,
        test_bytes_hex_literal,
        test_bytes_join,
        test_bytes_split,
        test_bytes_strip,
        test_bytes_startswith_endswith,
        test_bytes_find_index,
        test_bytes_count,
        test_bytes_replace,
        test_bytes_upper_lower,
        test_bytes_repr,
        test_bytes_iteration,
        test_bytearray_basic,
        test_bytearray_from_str,
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

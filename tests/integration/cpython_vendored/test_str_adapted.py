# Adapted from CPython 3.12.x cpython/Lib/test/test_unicode.py
# (In 3.12 the str-type test file is named test_unicode.py for historical reasons;
# it is the canonical exerciser for the str type.)
# Python Software Foundation License (PSF-2.0)
#
# Drop bucket — tests omitted:
#   * Encoding/decoding round-trips with errors handlers: test_codecs_*,
#     test_codecs_utf*_errors — depend on the codec registry.
#   * Surrogate / astral-plane edges: test_surrogates, test_astral.
#   * Format-spec deep dive: test_format_huge_*, test_formatting_huge_precision,
#     test_format_subclass, test_format_map.
#   * Locale-aware methods: test_islower, test_isupper at locale boundaries.
#   * PEP 393 / internal-representation tests.
#   * Translate/maketrans full sweep — kept the simple case.
#   * Hash-sensitive comparisons across normalization forms (NFC/NFD).

def test_str_constructor():
    assert str() == ''
    assert str('') == ''
    assert str('abc') == 'abc'
    assert str(42) == '42'
    assert str(1.5) == '1.5'
    assert str(True) == 'True'
    assert str(None) == 'None'
    assert str([1, 2, 3]) == '[1, 2, 3]'

def test_str_len():
    assert len('') == 0
    assert len('a') == 1
    assert len('hello') == 5

def test_str_indexing():
    s = 'hello'
    assert s[0] == 'h'
    assert s[4] == 'o'
    assert s[-1] == 'o'
    assert s[-5] == 'h'
    try:
        _ = s[5]
        raise AssertionError("expected IndexError")
    except IndexError:
        pass

def test_str_slicing():
    s = 'hello'
    assert s[1:4] == 'ell'
    assert s[:3] == 'hel'
    assert s[3:] == 'lo'
    assert s[:] == 'hello'
    assert s[::2] == 'hlo'
    assert s[::-1] == 'olleh'

def test_str_concatenation():
    assert 'abc' + 'def' == 'abcdef'
    assert '' + 'x' == 'x'
    s = 'foo'
    s += 'bar'
    assert s == 'foobar'

def test_str_repetition():
    assert 'ab' * 3 == 'ababab'
    assert 3 * 'ab' == 'ababab'
    assert 'x' * 0 == ''
    assert 'x' * -1 == ''

def test_str_contains():
    assert 'ell' in 'hello'
    assert 'xyz' not in 'hello'
    assert '' in 'hello'

def test_str_equality():
    assert 'abc' == 'abc'
    assert 'abc' != 'abd'
    assert '' == ''

def test_str_comparison():
    assert 'a' < 'b'
    assert 'aa' < 'ab'
    assert 'a' < 'aa'
    assert 'A' < 'a'
    assert 'b' > 'a'

def test_str_upper_lower():
    assert 'abc'.upper() == 'ABC'
    assert 'ABC'.lower() == 'abc'
    assert ''.upper() == ''
    assert 'Hello World'.upper() == 'HELLO WORLD'
    assert 'Hello World'.lower() == 'hello world'

def test_str_strip():
    assert '  abc  '.strip() == 'abc'
    assert 'xxabcyy'.strip('xy') == 'abc'
    assert '  abc  '.lstrip() == 'abc  '
    assert '  abc  '.rstrip() == '  abc'
    assert '\t\nabc\r\n'.strip() == 'abc'

def test_str_split():
    assert 'a,b,c'.split(',') == ['a', 'b', 'c']
    assert 'a b c'.split() == ['a', 'b', 'c']
    assert '  a  b  '.split() == ['a', 'b']
    assert 'a,b,c'.split(',', 1) == ['a', 'b,c']
    assert ''.split(',') == ['']

def test_str_rsplit():
    assert 'a,b,c'.rsplit(',', 1) == ['a,b', 'c']

def test_str_join():
    assert ','.join(['a', 'b', 'c']) == 'a,b,c'
    assert ''.join(['a', 'b', 'c']) == 'abc'
    assert ' '.join([]) == ''
    assert '-'.join(['x']) == 'x'

def test_str_replace():
    assert 'hello'.replace('l', 'L') == 'heLLo'
    assert 'hello'.replace('l', 'L', 1) == 'heLlo'
    assert 'hello'.replace('xyz', 'foo') == 'hello'
    assert ''.replace('a', 'b') == ''

def test_str_find_index():
    assert 'hello'.find('ll') == 2
    assert 'hello'.find('xx') == -1
    assert 'hello'.index('ll') == 2
    try:
        'hello'.index('xx')
        raise AssertionError("expected ValueError")
    except ValueError:
        pass
    assert 'hello'.rfind('l') == 3

def test_str_count():
    assert 'hello'.count('l') == 2
    assert 'hello'.count('xx') == 0
    assert 'hello'.count('') == 6  # len + 1

def test_str_startswith_endswith():
    assert 'hello'.startswith('he')
    assert not 'hello'.startswith('lo')
    assert 'hello'.endswith('lo')
    assert not 'hello'.endswith('he')
    assert 'hello'.startswith(('he', 'xx'))  # tuple form

def test_str_isdigit_isalpha():
    assert '123'.isdigit()
    assert not '12a'.isdigit()
    assert ''.isdigit() == False
    assert 'abc'.isalpha()
    assert not 'a1'.isalpha()
    assert 'abc123'.isalnum()
    assert ' '.isspace()
    assert '\t\n'.isspace()

def test_str_format():
    assert 'hello {}'.format('world') == 'hello world'
    assert '{0} {1}'.format('a', 'b') == 'a b'
    assert '{name}'.format(name='x') == 'x'
    assert '{:>5}'.format('a') == '    a'
    assert '{:<5}'.format('a') == 'a    '
    assert '{:.2f}'.format(3.14159) == '3.14'
    assert '{:d}'.format(42) == '42'

def test_str_fstring():
    x = 42
    assert f'x={x}' == 'x=42'
    name = 'world'
    assert f'hello {name}' == 'hello world'
    assert f'{1 + 2}' == '3'
    assert f'{3.14:.2f}' == '3.14'

def test_str_repr():
    assert repr('abc') == "'abc'"
    assert repr('') == "''"
    assert repr("a'b") == '"a\'b"'

def test_str_iteration():
    chars = []
    for c in 'abc':
        chars.append(c)
    assert chars == ['a', 'b', 'c']
    assert list('abc') == ['a', 'b', 'c']

def test_str_capitalize_title():
    assert 'hello world'.capitalize() == 'Hello world'
    assert 'hello world'.title() == 'Hello World'
    assert 'HELLO'.swapcase() == 'hello'


if __name__ == "__main__":
    run_tests = [
        test_str_constructor,
        test_str_len,
        test_str_indexing,
        test_str_slicing,
        test_str_concatenation,
        test_str_repetition,
        test_str_contains,
        test_str_equality,
        test_str_comparison,
        test_str_upper_lower,
        test_str_strip,
        test_str_split,
        test_str_rsplit,
        test_str_join,
        test_str_replace,
        test_str_find_index,
        test_str_count,
        test_str_startswith_endswith,
        test_str_isdigit_isalpha,
        test_str_format,
        test_str_fstring,
        test_str_repr,
        test_str_iteration,
        test_str_capitalize_title,
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

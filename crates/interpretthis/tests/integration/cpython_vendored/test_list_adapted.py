# Adapted from CPython 3.12.x cpython/Lib/test/test_list.py (and its parent
# test/list_tests.py / test/seq_tests.py base classes).
# Python Software Foundation License (PSF-2.0)
#
# Drop bucket — tests omitted:
#   * Pickle round-trips: test_iterator_pickle, test_reversed_pickle.
#   * sys.maxsize / overflow: test_overflow, test_list_resize_overflow, test_step_overflow.
#   * C-API / refcount: test_no_comdat_folding, test_preallocation, test_count_index_remove_crashes.
#   * Self-modifying operand semantics: test_equal_operator_modifying_operand,
#     test_lt_operator_modifying_operand, test_list_index_modifing_operand —
#     adversarial dunder reentrancy.
#   * Subclass keyword-arg dispatch: test_keywords_in_subclass.
#   * Most of the file inherits from list_tests.CommonTest / seq_tests.CommonTest;
#     we adapted the highest-signal canonical-semantics tests by hand rather than
#     vendoring the multi-thousand-line base classes.

def test_basic():
    assert list([]) == []
    l0_3 = [0, 1, 2, 3]
    l0_3_bis = list(l0_3)
    assert l0_3 == l0_3_bis
    assert l0_3 is not l0_3_bis
    assert list(()) == []
    assert list((0, 1, 2, 3)) == [0, 1, 2, 3]
    assert list('') == []
    assert list('spam') == ['s', 'p', 'a', 'm']
    assert list(x for x in range(10) if x % 2) == [1, 3, 5, 7, 9]

def test_truth():
    assert not []
    assert [42]

def test_identity():
    assert [] is not []

def test_len():
    assert len([]) == 0
    assert len([0]) == 1
    assert len([0, 1, 2]) == 3

def test_indexing():
    a = [0, 1, 2, 3, 4]
    assert a[0] == 0
    assert a[4] == 4
    assert a[-1] == 4
    assert a[-5] == 0
    try:
        _ = a[5]
        raise AssertionError("expected IndexError")
    except IndexError:
        pass

def test_slicing():
    a = [0, 1, 2, 3, 4]
    assert a[1:3] == [1, 2]
    assert a[:3] == [0, 1, 2]
    assert a[3:] == [3, 4]
    assert a[:] == [0, 1, 2, 3, 4]
    assert a[::2] == [0, 2, 4]
    assert a[::-1] == [4, 3, 2, 1, 0]
    assert a[1::2] == [1, 3]

def test_slice_assignment():
    a = [0, 1, 2, 3, 4]
    a[1:3] = [10, 20, 30]
    assert a == [0, 10, 20, 30, 3, 4]
    a = [0, 1, 2, 3, 4]
    a[::2] = [-1, -2, -3]
    assert a == [-1, 1, -2, 3, -3]

def test_append():
    a = []
    a.append(1)
    a.append(2)
    a.append(3)
    assert a == [1, 2, 3]

def test_extend():
    a = [1, 2]
    a.extend([3, 4])
    assert a == [1, 2, 3, 4]
    a.extend((5, 6))
    assert a == [1, 2, 3, 4, 5, 6]
    a.extend(x for x in range(7, 9))
    assert a == [1, 2, 3, 4, 5, 6, 7, 8]

def test_insert():
    a = [1, 2, 3]
    a.insert(0, 0)
    assert a == [0, 1, 2, 3]
    a.insert(2, -1)
    assert a == [0, 1, -1, 2, 3]
    a.insert(len(a), 99)
    assert a == [0, 1, -1, 2, 3, 99]

def test_pop():
    a = [1, 2, 3]
    assert a.pop() == 3
    assert a == [1, 2]
    assert a.pop(0) == 1
    assert a == [2]
    try:
        [].pop()
        raise AssertionError("expected IndexError")
    except IndexError:
        pass

def test_remove():
    a = [1, 2, 3, 2]
    a.remove(2)
    assert a == [1, 3, 2]
    try:
        a.remove(99)
        raise AssertionError("expected ValueError")
    except ValueError:
        pass

def test_index():
    a = [1, 2, 3, 2]
    assert a.index(2) == 1
    assert a.index(2, 2) == 3
    try:
        a.index(99)
        raise AssertionError("expected ValueError")
    except ValueError:
        pass

def test_count():
    a = [1, 2, 3, 2, 1, 2]
    assert a.count(1) == 2
    assert a.count(2) == 3
    assert a.count(99) == 0

def test_reverse():
    a = [1, 2, 3]
    a.reverse()
    assert a == [3, 2, 1]
    a = []
    a.reverse()
    assert a == []

def test_sort():
    a = [3, 1, 4, 1, 5, 9, 2, 6]
    a.sort()
    assert a == [1, 1, 2, 3, 4, 5, 6, 9]
    a.sort(reverse=True)
    assert a == [9, 6, 5, 4, 3, 2, 1, 1]
    a = ['banana', 'apple', 'cherry']
    a.sort()
    assert a == ['apple', 'banana', 'cherry']

def test_sort_key():
    a = ['banana', 'apple', 'cherry']
    a.sort(key=len)
    assert a == ['apple', 'banana', 'cherry']

def test_concat():
    assert [1, 2] + [3, 4] == [1, 2, 3, 4]
    a = [1, 2]
    a += [3]
    assert a == [1, 2, 3]

def test_repeat():
    assert [0] * 3 == [0, 0, 0]
    assert 3 * [0] == [0, 0, 0]
    assert [1, 2] * 2 == [1, 2, 1, 2]
    assert [] * 5 == []

def test_contains():
    a = [1, 2, 3]
    assert 1 in a
    assert 4 not in a

def test_equality():
    assert [1, 2, 3] == [1, 2, 3]
    assert [1, 2] != [1, 2, 3]
    assert [] == []

def test_copy_method():
    a = [1, 2, 3]
    b = a.copy()
    assert a == b
    assert a is not b
    b.append(4)
    assert a == [1, 2, 3]


if __name__ == "__main__":
    run_tests = [
        test_basic,
        test_truth,
        test_identity,
        test_len,
        test_indexing,
        test_slicing,
        test_slice_assignment,
        test_append,
        test_extend,
        test_insert,
        test_pop,
        test_remove,
        test_index,
        test_count,
        test_reverse,
        test_sort,
        test_sort_key,
        test_concat,
        test_repeat,
        test_contains,
        test_equality,
        test_copy_method,
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

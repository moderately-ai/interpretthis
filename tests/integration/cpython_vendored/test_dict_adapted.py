# Adapted from CPython 3.12.x cpython/Lib/test/test_dict.py
# Python Software Foundation License (PSF-2.0)
#
# Drop bucket — tests omitted from the original file:
#   * sys/gc/refcount internals: test_track_*, test_copy_maintains_tracking,
#     test_string_keys_can_track_values, test_container_iterator,
#     test_empty_presized_dict_in_freelist, test_resize* (CPython freelist mechanics).
#   * pickle round-trips: test_pickle_*, test_reduce*.
#   * unittest introspection: test_invalid_keyword_arguments uses with-assertRaises
#     in a way that requires a subTest-style fixture; the assertion model is
#     covered by the simpler test_constructor* tests we kept.
#   * Hash-randomization-dependent ordering: test_keys/test_values/test_items as
#     written test set(d.keys()) which is hash-order-dependent; we keep the
#     equality content but reroute through sorted() where order would diverge.
#   * Mutating-iteration tests: test_mutating_iteration*, test_mutating_lookup —
#     observe specific CPython invariants around RuntimeError mid-iteration; not
#     core dict semantics.
#   * BadEq/BadHash recursion tests: test_getitem's BadEq/BadHash exercise
#     hash-collision handling that depends on dunder propagation; kept the simple
#     KeyError-on-missing assertion, dropped the BadHash branch.
#   * test_views_mapping uses mappingproxy = type(type.__dict__) which depends on
#     class-dict implementation; not relevant to value semantics.

def test_constructor():
    assert dict() == {}
    assert dict() is not {}

def test_bool():
    assert not {}
    assert {1: 2}
    assert bool({}) is False
    assert bool({1: 2}) is True

def test_keys_basic():
    d = {}
    assert sorted(d.keys()) == []
    d = {'a': 1, 'b': 2}
    assert sorted(d.keys()) == ['a', 'b']
    assert 'a' in d
    assert 'b' in d

def test_values_basic():
    d = {}
    assert sorted(d.values()) == []
    d = {1: 2}
    assert sorted(d.values()) == [2]

def test_items_basic():
    d = {}
    assert sorted(d.items()) == []
    d = {1: 2}
    assert sorted(d.items()) == [(1, 2)]

def test_contains():
    d = {}
    assert 'a' not in d
    assert not ('a' in d)
    assert 'a' not in d
    d = {'a': 1, 'b': 2}
    assert 'a' in d
    assert 'b' in d
    assert 'c' not in d

def test_len():
    d = {}
    assert len(d) == 0
    d = {'a': 1, 'b': 2}
    assert len(d) == 2

def test_getitem():
    d = {'a': 1, 'b': 2}
    assert d['a'] == 1
    assert d['b'] == 2
    d['c'] = 3
    d['a'] = 4
    assert d['c'] == 3
    assert d['a'] == 4
    del d['b']
    assert d == {'a': 4, 'c': 3}
    try:
        _ = d['missing']
        raise AssertionError("expected KeyError")
    except KeyError:
        pass

def test_clear():
    d = {1: 1, 2: 2, 3: 3}
    d.clear()
    assert d == {}

def test_update():
    d = {}
    d.update({1: 100})
    d.update({2: 20})
    d.update({1: 1, 2: 2, 3: 3})
    assert d == {1: 1, 2: 2, 3: 3}
    d.update()
    assert d == {1: 1, 2: 2, 3: 3}

def test_fromkeys():
    assert dict.fromkeys('abc') == {'a': None, 'b': None, 'c': None}
    d = {}
    assert d.fromkeys('abc') is not d
    assert d.fromkeys('abc') == {'a': None, 'b': None, 'c': None}
    assert d.fromkeys((4, 5), 0) == {4: 0, 5: 0}
    assert d.fromkeys([]) == {}

def test_copy():
    d = {1: 1, 2: 2, 3: 3}
    assert d.copy() == {1: 1, 2: 2, 3: 3}
    assert {}.copy() == {}
    assert d.copy() is not d

def test_get():
    d = {}
    assert d.get('c') is None
    assert d.get('c', 3) == 3
    d = {'a': 1, 'b': 2}
    assert d.get('c') is None
    assert d.get('c', 3) == 3
    assert d.get('a') == 1
    assert d.get('a', 3) == 1

def test_setdefault():
    d = {}
    assert d.setdefault('key0') is None
    d.setdefault('key0', [])
    assert d.setdefault('key0') is None
    d.setdefault('key', []).append(3)
    assert d['key'][0] == 3
    d.setdefault('key', []).append(4)
    assert len(d['key']) == 2

def test_popitem():
    for copymode in -1, +1:
        for log2size in range(12):
            size = 2 ** log2size
            a = {}
            b = {}
            for i in range(size):
                a[repr(i)] = i
                if copymode < 0:
                    b[repr(i)] = i
            if copymode > 0:
                b = a.copy()
            for i in range(size):
                ka, va = ta = a.popitem()
                assert va == int(ka)
                kb, vb = tb = b.popitem()
                assert vb == int(kb)
            assert not a
            assert not b

def test_pop():
    d = {}
    k, v = 'abc', 'def'
    d[k] = v
    try:
        d.pop('ghi')
        raise AssertionError("expected KeyError")
    except KeyError:
        pass
    assert d.pop(k) == v
    assert len(d) == 0
    try:
        d.pop(k)
        raise AssertionError("expected KeyError")
    except KeyError:
        pass
    assert d.pop(k, v) == v
    d[k] = v
    assert d.pop(k, 1) == v

def test_eq():
    assert {} == {}
    assert {1: 2} == {1: 2}

def test_keys_insertion_order():
    d = {}
    d['a'] = 1
    d['b'] = 2
    d['c'] = 3
    assert list(d.keys()) == ['a', 'b', 'c']
    assert list(d.values()) == [1, 2, 3]
    assert list(d.items()) == [('a', 1), ('b', 2), ('c', 3)]

def test_bool_int_key_unification():
    # CPython: bool subclasses int; True hashes to 1, False to 0.
    # {True: x}[1] returns x; {1: y, True: z} collapses to {1: z}.
    assert {True: 'x'}[1] == 'x'
    assert {1: 'y', True: 'z'} == {1: 'z'}
    assert len({1: 'y', True: 'z'}) == 1

def test_merge_operator():
    # PEP 584 — dict | dict and dict |= dict.
    a = {1: 1, 2: 2}
    b = {2: 20, 3: 30}
    assert a | b == {1: 1, 2: 20, 3: 30}
    assert b | a == {1: 1, 2: 2, 3: 30}
    a |= b
    assert a == {1: 1, 2: 20, 3: 30}


if __name__ == "__main__":
    run_tests = [
        test_constructor,
        test_bool,
        test_keys_basic,
        test_values_basic,
        test_items_basic,
        test_contains,
        test_len,
        test_getitem,
        test_clear,
        test_update,
        test_fromkeys,
        test_copy,
        test_get,
        test_setdefault,
        test_popitem,
        test_pop,
        test_eq,
        test_keys_insertion_order,
        test_bool_int_key_unification,
        test_merge_operator,
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

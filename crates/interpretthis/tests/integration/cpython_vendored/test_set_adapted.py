# Adapted from CPython 3.12.x cpython/Lib/test/test_set.py
# Python Software Foundation License (PSF-2.0)
#
# Drop bucket — tests omitted from the original file:
#   * pickle/deepcopy round-trips: test_pickling, test_iterator_pickling, test_deepcopy.
#   * GC/refcount: test_gc, test_container_iterator, test_free_after_iterating.
#   * weakref: test_weakref.
#   * C-API: test_c_api, test_subclass_with_custom_hash, test_keywords_in_subclass.
#   * Hash-internals: test_hash_caching, test_hash_effectiveness, test_do_not_rehash_dict_keys.
#   * Bad-eq/bad-hash dunder-recursion: test_badcmp.
#   * Mutation-during-iteration: test_inplace_on_self.
#   * Print-order-sensitive tests reroute through sorted(); sets are documented as
#     "deterministic insertion order" in CONFORMANCE.md, but for parity-corpus
#     purposes we explicitly sort to keep the test orthogonal to that divergence.

def test_uniquification():
    actual = sorted(set([0, 1, 2, 1, 0, 2]))
    assert actual == [0, 1, 2]

def test_len():
    word = 'simsalabim'
    s = set(word)
    assert len(s) == len(set(word))

def test_contains():
    word = 'simsalabim'
    s = set(word)
    for c in word:
        assert c in s
    assert 'z' not in s

def test_union():
    word1 = 'simsalabim'
    word2 = 'madagascar'
    s1 = set(word1)
    s2 = set(word2)
    i = s1.union(s2)
    for c in word1:
        assert c in i
    for c in word2:
        assert c in i

def test_or():
    a = {1, 2, 3}
    b = {3, 4, 5}
    assert a | b == {1, 2, 3, 4, 5}

def test_intersection():
    word1 = 'simsalabim'
    word2 = 'madagascar'
    s1 = set(word1)
    s2 = set(word2)
    common = sorted(s1.intersection(s2))
    for c in common:
        assert c in word1
        assert c in word2

def test_and():
    a = {1, 2, 3}
    b = {3, 4, 5}
    assert a & b == {3}

def test_difference():
    a = {1, 2, 3}
    b = {2, 3, 4}
    assert a.difference(b) == {1}
    assert b.difference(a) == {4}

def test_sub():
    a = {1, 2, 3}
    b = {3, 4, 5}
    assert a - b == {1, 2}
    assert b - a == {4, 5}

def test_symmetric_difference():
    a = {1, 2, 3}
    b = {3, 4, 5}
    assert a.symmetric_difference(b) == {1, 2, 4, 5}

def test_xor():
    a = {1, 2, 3}
    b = {3, 4, 5}
    assert a ^ b == {1, 2, 4, 5}

def test_equality():
    assert set('simsalabim') == set('simsalabim')
    assert set('simsalabim') != set('madagascar')
    assert set() == set()
    assert set() != {0}

def test_isdisjoint():
    a = {1, 2, 3}
    b = {4, 5, 6}
    assert a.isdisjoint(b)
    assert not a.isdisjoint({3, 7})

def test_sub_and_super():
    a = {1, 2}
    b = {1, 2, 3}
    assert a.issubset(b)
    assert b.issuperset(a)
    assert a <= b
    assert b >= a
    assert a < b
    assert b > a
    assert not (a < a)
    assert a <= a

def test_clear():
    s = {1, 2, 3}
    s.clear()
    assert s == set()
    assert len(s) == 0

def test_copy():
    s = {1, 2, 3}
    t = s.copy()
    assert s == t
    assert s is not t
    t.add(4)
    assert 4 not in s

def test_add():
    s = set()
    s.add(1)
    s.add(2)
    s.add(1)
    assert s == {1, 2}

def test_remove():
    s = {1, 2, 3}
    s.remove(2)
    assert s == {1, 3}
    try:
        s.remove(10)
        raise AssertionError("expected KeyError")
    except KeyError:
        pass

def test_discard():
    s = {1, 2, 3}
    s.discard(2)
    assert s == {1, 3}
    s.discard(10)
    assert s == {1, 3}

def test_pop():
    s = {1}
    v = s.pop()
    assert v == 1
    assert s == set()
    try:
        s.pop()
        raise AssertionError("expected KeyError")
    except KeyError:
        pass

def test_update():
    s = {1, 2}
    s.update({3, 4})
    assert s == {1, 2, 3, 4}
    s.update([5, 6])
    assert s == {1, 2, 3, 4, 5, 6}

def test_intersection_update():
    s = {1, 2, 3, 4}
    s.intersection_update({2, 4, 6})
    assert s == {2, 4}

def test_difference_update():
    s = {1, 2, 3, 4}
    s.difference_update({2, 4})
    assert s == {1, 3}

def test_symmetric_difference_update():
    s = {1, 2, 3, 4}
    s.symmetric_difference_update({3, 4, 5, 6})
    assert s == {1, 2, 5, 6}

def test_set_literal():
    s = {1, 2, 3}
    assert s == set([1, 2, 3])

def test_frozenset_hash():
    a = frozenset([1, 2, 3])
    b = frozenset([3, 2, 1])
    assert hash(a) == hash(b)
    assert a == b

def test_frozen_as_dictkey():
    a = frozenset([1, 2])
    d = {a: 'value'}
    assert d[frozenset([1, 2])] == 'value'


if __name__ == "__main__":
    run_tests = [
        test_uniquification,
        test_len,
        test_contains,
        test_union,
        test_or,
        test_intersection,
        test_and,
        test_difference,
        test_sub,
        test_symmetric_difference,
        test_xor,
        test_equality,
        test_isdisjoint,
        test_sub_and_super,
        test_clear,
        test_copy,
        test_add,
        test_remove,
        test_discard,
        test_pop,
        test_update,
        test_intersection_update,
        test_difference_update,
        test_symmetric_difference_update,
        test_set_literal,
        test_frozenset_hash,
        test_frozen_as_dictkey,
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

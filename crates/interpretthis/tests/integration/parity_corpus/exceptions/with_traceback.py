# BaseException.with_traceback(tb) returns the exception (we don't model
# tracebacks, so tb is accepted and ignored); __traceback__ reads as None.

e = Exception("a").with_traceback(None)
print(e)

try:
    raise ValueError("boom").with_traceback(None)
except ValueError as ve:
    print("caught", ve)

err = KeyError("k")
print(err.with_traceback(None))
print(err.__traceback__)


def reraise():
    try:
        raise RuntimeError("orig")
    except RuntimeError as exc:
        raise exc.with_traceback(exc.__traceback__)


try:
    reraise()
except RuntimeError as exc:
    print("re:", exc)

# with_traceback is also reachable as a bound method value.
m = ValueError("v").with_traceback
print(m(None))

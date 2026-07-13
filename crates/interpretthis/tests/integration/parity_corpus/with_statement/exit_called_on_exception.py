# Pins: __exit__ is called even when the body raises, with the
# exception info passed as args. Returning False from __exit__
# does NOT suppress; the exception propagates.
class CM:
    def __enter__(self):
        return self
    def __exit__(self, exc_type, exc_value, traceback):
        # CPython passes the exception class, instance, and traceback.
        # We don't model traceback objects yet — print the class name
        # only to keep the output deterministic across engines.
        if exc_type is None:
            print('exit no error')
        else:
            print(f'exit caught {exc_type.__name__}')
        return False

try:
    with CM():
        raise ValueError('boom')
except ValueError:
    print('outer caught ValueError')

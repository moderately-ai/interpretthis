# Pins: `with` statement on a user class with __enter__/__exit__.
# Most basic context manager pattern — __enter__ runs before body,
# __exit__ runs after.
class CM:
    def __enter__(self):
        print('enter')
        return 'hello'
    def __exit__(self, exc_type, exc_value, traceback):
        print('exit')
        return False

with CM() as msg:
    print(msg)
print('after')

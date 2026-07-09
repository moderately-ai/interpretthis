# Pins: `with A, B:` enters in declaration order, exits in REVERSE
# order — CPython's protocol so nested resources unwind cleanly.
class CM:
    def __init__(self, label):
        self.label = label
    def __enter__(self):
        print(f'enter {self.label}')
        return self
    def __exit__(self, exc_type, exc_value, traceback):
        print(f'exit {self.label}')
        return False

with CM('A'), CM('B'), CM('C'):
    print('body')

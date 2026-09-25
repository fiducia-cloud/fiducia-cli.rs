#!/usr/bin/env python3
from itertools import product

def allowed(target: bool, confirmed: bool, fresh_auth: bool, destructive: bool) -> bool:
    if not destructive:
        return target
    return target and confirmed and fresh_auth

def main() -> None:
    explored = 0
    for target, confirmed, fresh_auth, destructive in product((False, True), repeat=4):
        explored += 1
        ok = allowed(target, confirmed, fresh_auth, destructive)
        if destructive and ok:
            assert target and confirmed and fresh_auth
    assert not allowed(True, False, True, True)
    assert not allowed(True, True, False, True)
    print(f'CLI destructive admission: {explored} states')

if __name__ == '__main__':
    main()

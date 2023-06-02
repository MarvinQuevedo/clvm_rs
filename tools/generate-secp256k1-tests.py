from secp256k1 import PublicKey, PrivateKey
from hashlib import sha256
from random import randbytes, randint, seed, sample

def flip_bit(b: bytes) -> bytearray:
    idx = randint(0, len(b) - 1)
    bit = 1 << randint(0, 7)
    ret = bytearray(b)
    ret[idx] ^= bit
    return ret

def print_validation_test_case(f, num_cases, filter_pk, filter_msg, filter_sig, expect: str):
    sks = sample(secret_keys, num_cases)
    cost = 850000
    sigs = []

    args = ""
    for sk in sks:
        pk = sk.pubkey
        msg = randbytes(randint(3,40))
        sig = sk.ecdsa_sign(msg)
        sha = sha256()
        sha.update(msg)
        f.write(f"secp256k1_verify 0x{bytes(filter_pk(pk.serialize())).hex()} 0x{filter_msg(sha.digest()).hex()} 0x{bytes(filter_sig(sk.ecdsa_serialize_compact(sig))).hex()}")

        f.write(f" => {expect}")
        if expect != "FAIL":
            f.write(f" | {cost}")
        f.write("\n")


seed(1337)

SIZE = 30

# generate a bunch of keys
secret_keys = []
for i in range(SIZE):
    secret_keys.append(PrivateKey())


with open("../op-tests/test-secp256k1.txt", "w+") as f:
    f.write("; This file was generated by tools/generate-secp256k1-tests.py\n\n")

    print_validation_test_case(f, SIZE, lambda pk: pk, lambda msg: msg, lambda sig: sig, "0")

    # negative tests (alter public key)
    print_validation_test_case(f, 3, flip_bit, lambda msg: msg, lambda sig: sig, "FAIL")

    # negative tests (alter message)
    print_validation_test_case(f, 3, lambda pk: pk, flip_bit, lambda sig: sig, "FAIL")

    # negative tests (alter signature)
    print_validation_test_case(f, 3, lambda pk: pk, lambda msg: msg, flip_bit, "FAIL")

defmodule HashidsSteps do
  @moduledoc """
  Companion steps for hashids.tast — real Hashids library calls.

  Each function corresponds to a node in the HashidsPipeline graph.
  Receives a map of string-keyed inputs from upstream steps (via TAST_INPUT env vars),
  returns a map of string-keyed outputs for downstream steps (via TAST_OUTPUT markers).

  Since Elixir structs can't pass through TAST's string-based data flow, downstream
  steps reconstruct the Hashids struct from the salt string received as input.
  Numeric lists are serialized as Elixir terms via `inspect/1` and deserialized
  with `Code.eval_string/1`.
  """

  import ExUnit.Assertions

  # Test fixtures — used only by the root node (BuildConfig).
  @salt "my salt"
  @numbers [1, 2, 3]
  @min_len 25

  # ── Root node: no upstream inputs ──────────────────────────

  def build_config(_inputs) do
    h = Hashids.new(salt: @salt)

    # the struct contains the parsed salt as a charlist
    assert is_list(h.salt), "salt should be a charlist"
    assert h.salt == String.to_charlist(@salt), "salt should match input"

    # the alphabet is uniquified and validated for minimum length
    assert length(h.alphabet) >= 16, "alphabet must have at least 16 unique chars"

    %{
      "salt" => @salt,
      "alphabet" => List.to_string(h.alphabet),
      "min_len" => "0"
    }
  end

  # ── Receives salt, alphabet, min_len from BuildConfig ──────

  def prepare_alphabet(inputs) do
    salt = inputs["salt"]
    assert salt != nil, "should receive salt from BuildConfig"

    h = Hashids.new(salt: salt)

    # the remaining alphabet is consistently shuffled using the salt
    assert is_list(h.alphabet), "alphabet should be a charlist"
    assert length(h.alphabet) > 0, "alphabet should not be empty"

    # guard characters are split from the front of the alphabet or separators
    assert is_list(h.guards), "guards should be a charlist"
    assert length(h.guards) > 0, "guards should not be empty"

    # the separator-to-alphabet ratio is enforced
    assert is_list(h.seps), "seps should be a charlist"
    assert length(h.seps) > 0, "seps should not be empty"

    %{
      "shuffled_alphabet" => List.to_string(h.alphabet),
      "separators" => List.to_string(h.seps),
      "guards" => List.to_string(h.guards)
    }
  end

  # ── Receives shuffled_alphabet, separators, guards from PrepareAlphabet ──

  def encode_numbers(inputs) do
    assert inputs["shuffled_alphabet"] != nil, "should receive shuffled_alphabet from PrepareAlphabet"
    assert inputs["separators"] != nil, "should receive separators from PrepareAlphabet"
    assert inputs["guards"] != nil, "should receive guards from PrepareAlphabet"

    # Reconstruct struct from the salt that produced the received alphabet.
    h = Hashids.new(salt: @salt)

    # Verify the struct's alphabet matches what we received.
    assert List.to_string(h.alphabet) == inputs["shuffled_alphabet"],
           "reconstructed alphabet should match upstream"

    encoded = Hashids.encode(h, @numbers)

    # a lottery character is selected and each number is encoded
    assert is_binary(encoded), "encoded result should be a binary string"
    assert byte_size(encoded) > 0, "encoded result should be non-empty"

    %{
      "hash_string" => encoded,
      "hashids_struct" => @salt
    }
  end

  # ── Receives hash_string, hashids_struct from EncodeNumbers ──

  def decode_hash(inputs) do
    hash_string = inputs["hash_string"]
    salt = inputs["hashids_struct"]
    assert hash_string != nil, "should receive hash_string from EncodeNumbers"
    assert salt != nil, "should receive hashids_struct (salt) from EncodeNumbers"

    # Reconstruct struct from the salt and decode the received hash.
    h = Hashids.new(salt: salt)
    {:ok, decoded} = Hashids.decode(h, hash_string)

    # each segment is decoded back to its original number
    assert is_list(decoded), "decoded result should be a list"
    assert decoded == @numbers, "decoded should match original numbers"

    %{
      "decoded_numbers" => inspect(decoded),
      "original_numbers" => inspect(@numbers),
      "hash_string" => hash_string
    }
  end

  # ── Receives decoded_numbers, original_numbers, hash_string from DecodeHash ──

  def verify_round_trip(inputs) do
    decoded_str = inputs["decoded_numbers"]
    original_str = inputs["original_numbers"]
    hash_string = inputs["hash_string"]
    assert decoded_str != nil, "should receive decoded_numbers from DecodeHash"
    assert original_str != nil, "should receive original_numbers from DecodeHash"
    assert hash_string != nil, "should receive hash_string from DecodeHash"

    # Deserialize the number lists from their inspected string form.
    {decoded, _} = Code.eval_string(decoded_str)
    {original, _} = Code.eval_string(original_str)

    # they are identical
    assert decoded == original, "round-trip: decoded should equal original"

    # re-encoding the decoded numbers produces the same hash string
    h = Hashids.new(salt: @salt)
    re_encoded = Hashids.encode(h, decoded)
    assert re_encoded == hash_string, "round-trip: re-encoded should equal original hash"

    %{}
  end

  # ── Receives shuffled_alphabet, separators, guards, min_len from PrepareAlphabet ──

  def encode_with_min_length(inputs) do
    assert inputs["shuffled_alphabet"] != nil, "should receive shuffled_alphabet from PrepareAlphabet"
    assert inputs["guards"] != nil, "should receive guards from PrepareAlphabet"

    h = Hashids.new(salt: @salt, min_len: @min_len)
    encoded = Hashids.encode(h, @numbers)

    # the output is padded with guards and alphabet halves to meet min_len
    assert byte_size(encoded) >= @min_len,
           "padded hash should be at least #{@min_len} chars, got #{byte_size(encoded)}"

    # the padding preserves decodability
    {:ok, decoded} = Hashids.decode(h, encoded)
    assert decoded == @numbers, "padded hash should decode correctly"

    %{
      "padded_hash" => encoded,
      "original_numbers" => inspect(@numbers),
      "min_len" => Integer.to_string(@min_len)
    }
  end

  # ── Receives padded_hash, original_numbers, min_len from EncodeWithMinLength ──

  def verify_padding(inputs) do
    padded_hash = inputs["padded_hash"]
    original_str = inputs["original_numbers"]
    min_len_str = inputs["min_len"]
    assert padded_hash != nil, "should receive padded_hash from EncodeWithMinLength"
    assert original_str != nil, "should receive original_numbers from EncodeWithMinLength"
    assert min_len_str != nil, "should receive min_len from EncodeWithMinLength"

    min_len = String.to_integer(min_len_str)
    {original, _} = Code.eval_string(original_str)

    # it is at least min_len characters long
    assert byte_size(padded_hash) >= min_len,
           "padded hash length should be >= #{min_len}, got #{byte_size(padded_hash)}"

    # decoding the padded hash returns the original numbers
    h = Hashids.new(salt: @salt, min_len: min_len)
    {:ok, decoded} = Hashids.decode(h, padded_hash)
    assert decoded == original, "padded hash should round-trip correctly"

    %{}
  end
end

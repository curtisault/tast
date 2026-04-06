defmodule SlugifySteps do
  @moduledoc """
  Companion steps for slugify.tast — real Slug library calls.

  Each function corresponds to a node in the SlugifyPipeline graph.
  Receives a map of string-keyed inputs from upstream steps,
  returns a map of string-keyed outputs for downstream steps.

  The pipeline processes "Hello, World!" through slugify's full
  transformation chain, validating each stage independently.
  Complex Elixir terms (lists, charlists) are serialized as
  simple joined strings to survive TAST's JSON data flow.
  """

  import ExUnit.Assertions

  @input "Hello, World!"
  @separator "-"

  # ── Root node ──────────────────────────────────────────────

  def parse_options(_inputs) do
    separator = @separator
    ignored_codepoints = String.to_charlist(separator)

    assert is_binary(separator)
    assert is_list(ignored_codepoints)
    assert Enum.member?(ignored_codepoints, ?-)

    %{
      "separator" => separator,
      "ignored_codepoints" => List.to_string(ignored_codepoints),
      "truncate_length" => "nil",
      "lowercase_flag" => "true"
    }
  end

  # ── Receives separator, ignored_codepoints, truncate_length, lowercase_flag ──

  def normalize_input(inputs) do
    assert inputs["separator"] != nil, "should receive separator from ParseOptions"

    graphemes = String.graphemes(@input)

    assert is_list(graphemes)
    assert length(graphemes) == String.length(@input)

    # Join graphemes with a delimiter that won't appear in normal text.
    %{
      "graphemes" => Enum.join(graphemes, "|")
    }
  end

  # ── Receives graphemes ─────────────────────────────────────

  def transliterate(inputs) do
    graphemes_str = inputs["graphemes"]
    assert graphemes_str != nil, "should receive graphemes from NormalizeInput"

    graphemes = String.split(graphemes_str, "|")
    ignored_codepoints = String.to_charlist(@separator)

    transliterated = Enum.map_join(graphemes, fn g ->
      codepoints = String.to_charlist(g)
      Enum.map_join(codepoints, fn cp ->
        cond do
          cp in ?A..?Z or cp in ?a..?z or cp in ?0..?9 -> <<cp>>
          cp in ignored_codepoints -> <<cp::utf8>>
          cp == ?\s -> " "
          true -> ""
        end
      end)
    end)

    assert is_binary(transliterated)
    assert String.contains?(transliterated, "Hello")

    %{
      "transliterated_string" => transliterated
    }
  end

  # ── Receives transliterated_string ─────────────────────────

  def clean_punctuation(inputs) do
    transliterated = inputs["transliterated_string"]
    assert transliterated != nil, "should receive transliterated_string from Transliterate"

    cleaned = transliterated
      |> String.replace("'", "")
      |> String.replace("`", "")

    refute String.contains?(cleaned, "'")
    refute String.contains?(cleaned, "`")

    %{
      "cleaned_string" => cleaned,
      "delimiter_regex" => @separator
    }
  end

  # ── Receives cleaned_string, delimiter_regex ───────────────

  def split_on_delimiters(inputs) do
    cleaned = inputs["cleaned_string"]
    separator = inputs["delimiter_regex"]
    assert cleaned != nil, "should receive cleaned_string from CleanPunctuation"
    assert separator != nil, "should receive delimiter_regex from CleanPunctuation"

    punctuation = ~c"!\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~"
    ignored = String.to_charlist(separator)
    regex_chars = punctuation |> Enum.filter(&(&1 not in ignored)) |> to_string()
    regex_str = "[" <> Regex.escape(separator) <> Regex.escape(regex_chars) <> "[:space:]]"
    regex = Regex.compile!(regex_str, [:unicode])

    segments = cleaned |> String.split(regex, trim: true) |> Enum.filter(&(&1 != ""))

    assert length(segments) > 0
    assert "Hello" in segments
    assert "World" in segments

    # Join with tab — no quotes, no JSON issues.
    %{
      "word_segments" => Enum.join(segments, "|")
    }
  end

  # ── Receives word_segments ─────────────────────────────────

  def truncate_to_word_boundary(inputs) do
    segments_str = inputs["word_segments"]
    assert segments_str != nil, "should receive word_segments from SplitOnDelimiters"

    segments = String.split(segments_str, "|")
    joined = Enum.join(segments, @separator)

    assert joined == "Hello-World"

    %{
      "joined_slug" => joined
    }
  end

  # ── Receives joined_slug ───────────────────────────────────

  def apply_lowercase(inputs) do
    joined = inputs["joined_slug"]
    assert joined != nil, "should receive joined_slug from TruncateToWordBoundary"

    lowered = String.downcase(joined)

    refute String.match?(lowered, ~r/[A-Z]/)
    assert lowered == "hello-world"

    %{
      "final_slug" => lowered
    }
  end

  # ── Receives final_slug ────────────────────────────────────

  def validate_output(inputs) do
    final_slug = inputs["final_slug"]
    assert final_slug != nil, "should receive final_slug from ApplyLowercase"

    assert final_slug != ""

    expected = Slug.slugify(@input)
    assert final_slug == expected,
           "step-by-step result '#{final_slug}' should match Slug.slugify/1 result '#{expected}'"

    %{}
  end
end

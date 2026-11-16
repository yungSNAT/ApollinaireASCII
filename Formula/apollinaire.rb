class Apollinaire < Formula
  desc "Static art, block lettering and animation as real ASCII text"
  homepage "https://yungsnat.github.io/ApollinaireASCII/"
  version "0.4.1"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/yungSNAT/ApollinaireASCII/releases/download/v0.4.1/apollinaire-v0.4.1-aarch64-apple-darwin.tar.gz"
      sha256 "1acdcf9a475418dc2fcea98ac6fe21854307a0280c9e1378aa162930cf3a350f"
    end
    on_intel do
      url "https://github.com/yungSNAT/ApollinaireASCII/releases/download/v0.4.1/apollinaire-v0.4.1-x86_64-apple-darwin.tar.gz"
      sha256 "a8bdb1ac6fd6a559ce6670036e39f15c81161ba067ed52eabaeaed8132336727"
    end
  end
  on_linux do
    on_arm do
      url "https://github.com/yungSNAT/ApollinaireASCII/releases/download/v0.4.1/apollinaire-v0.4.1-aarch64-unknown-linux-musl.tar.gz"
      sha256 "19e036be918fc85b878e635e10336d145cf7acfcd218d848b902524b0d88b1ab"
    end
    on_intel do
      url "https://github.com/yungSNAT/ApollinaireASCII/releases/download/v0.4.1/apollinaire-v0.4.1-x86_64-unknown-linux-musl.tar.gz"
      sha256 "68ec4834aff087e6444b4558ad5e14da2670b02adbcc79b9a03b5843da71ef0e"
    end
  end

  def install
    bin.install "apollinaire"
    man1.install "man/apollinaire.1"
    bash_completion.install "completions/apollinaire.bash" => "apollinaire"
    zsh_completion.install "completions/_apollinaire"
    fish_completion.install "completions/apollinaire.fish"
    doc.install "README.md", "THIRD_PARTY_NOTICES.md", "DEPENDENCY_NOTICES.txt", "LICENSES"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/apollinaire --version")
  end
end

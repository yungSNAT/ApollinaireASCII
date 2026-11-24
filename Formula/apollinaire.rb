class Apollinaire < Formula
  desc "Static art, block lettering and animation as real ASCII text"
  homepage "https://yungsnat.github.io/ApollinaireASCII/"
  version "0.4.2"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/yungSNAT/ApollinaireASCII/releases/download/v0.4.2/apollinaire-v0.4.2-aarch64-apple-darwin.tar.gz"
      sha256 "43a43eebde11dd5b3872a055ad2c16988fb3c2907902f574df0f435d1d66ccc4"
    end
    on_intel do
      url "https://github.com/yungSNAT/ApollinaireASCII/releases/download/v0.4.2/apollinaire-v0.4.2-x86_64-apple-darwin.tar.gz"
      sha256 "804ad2830975ebd572f16f74ef23b6ed1ba3416e0c7f3602ee72a8a6c6ed6bbf"
    end
  end
  on_linux do
    on_arm do
      url "https://github.com/yungSNAT/ApollinaireASCII/releases/download/v0.4.2/apollinaire-v0.4.2-aarch64-unknown-linux-musl.tar.gz"
      sha256 "c917f0a2efe2ce0c7029bed4d3e776c701f83647aaa407eac3104335fe292845"
    end
    on_intel do
      url "https://github.com/yungSNAT/ApollinaireASCII/releases/download/v0.4.2/apollinaire-v0.4.2-x86_64-unknown-linux-musl.tar.gz"
      sha256 "fcad798f5b61e17c2ee1763133362055ffeec0dc6d4a134bdbb93f327b42cfbf"
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

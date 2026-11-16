class Apollinaire < Formula
  desc "Static art, block lettering and animation as real ASCII text"
  homepage "https://yungsnat.github.io/ApollinaireASCII/"
  version "0.4.1"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/yungSNAT/ApollinaireASCII/releases/download/v0.4.1/apollinaire-v0.4.1-aarch64-apple-darwin.tar.gz"
      sha256 "726c49037ff398d32f30aec21b018d0fe68092e143889bef213e666f1c9162ff"
    end
    on_intel do
      url "https://github.com/yungSNAT/ApollinaireASCII/releases/download/v0.4.1/apollinaire-v0.4.1-x86_64-apple-darwin.tar.gz"
      sha256 "9939585f3a423b81185e807d1e049919d2bf6a66982c8e6e5e072420982005c8"
    end
  end
  on_linux do
    on_arm do
      url "https://github.com/yungSNAT/ApollinaireASCII/releases/download/v0.4.1/apollinaire-v0.4.1-aarch64-unknown-linux-musl.tar.gz"
      sha256 "aa2b036074b7f192bf13d28c5ffc5bb0f60f958db09825277348814bf75bff1c"
    end
    on_intel do
      url "https://github.com/yungSNAT/ApollinaireASCII/releases/download/v0.4.1/apollinaire-v0.4.1-x86_64-unknown-linux-musl.tar.gz"
      sha256 "c503ae39c15e78e71ff5cba2ff52a7e0175fc7ee57bd319fb2a79f2b902ef2f1"
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

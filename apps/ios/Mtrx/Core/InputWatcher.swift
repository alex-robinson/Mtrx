import SwiftUI
import UIKit
import UIKit.UIGestureRecognizerSubclass

/// Calls `onInput` at each touch or key press anywhere in its window
/// (sheets included), and as text is typed, without taking any from the
/// views: for whether the user is idle.
struct InputWatcher: UIViewRepresentable {
    let onInput: () -> Void

    func makeUIView(context: Context) -> WatcherView {
        WatcherView()
    }

    func updateUIView(_ view: WatcherView, context: Context) {
        view.recognizer.onInput = onInput
    }

    /// Puts its recognizer on the window it is in.
    final class WatcherView: UIView {
        let recognizer = InputRecognizer()
        private var typing: [NSObjectProtocol] = []

        override init(frame: CGRect) {
            super.init(frame: frame)
            isUserInteractionEnabled = false
            // The keyboard is a window of its own: typing shows as text
            // changing.
            let names = [UITextField.textDidChangeNotification, UITextView.textDidChangeNotification]
            typing = names.map { name in
                NotificationCenter.default.addObserver(forName: name, object: nil, queue: .main) { [weak self] _ in
                    MainActor.assumeIsolated { self?.recognizer.onInput() }
                }
            }
        }

        required init?(coder: NSCoder) {
            fatalError("init(coder:) is not used")
        }

        deinit {
            for observer in typing {
                NotificationCenter.default.removeObserver(observer)
            }
        }

        override func willMove(toWindow newWindow: UIWindow?) {
            super.willMove(toWindow: newWindow)
            recognizer.view?.removeGestureRecognizer(recognizer)
            newWindow?.addGestureRecognizer(recognizer)
        }
    }
}

/// Reports the start of each touch or press and fails at once, so that it
/// never recognizes, cancels or delays anything.
final class InputRecognizer: UIGestureRecognizer {
    var onInput: () -> Void = {}

    init() {
        super.init(target: nil, action: nil)
        cancelsTouchesInView = false
        delaysTouchesBegan = false
        delaysTouchesEnded = false
    }

    override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent) {
        onInput()
        state = .failed
    }

    override func pressesBegan(_ presses: Set<UIPress>, with event: UIPressesEvent) {
        onInput()
        state = .failed
    }
}

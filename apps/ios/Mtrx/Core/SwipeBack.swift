import UIKit

/// Swiping from the left edge goes back also where a screen has a back
/// button of its own (`navigationBarBackButtonHidden`), which otherwise
/// turns the gesture off: a room's in the Rooms tab, with its unread count.
extension UINavigationController: @retroactive UIGestureRecognizerDelegate {
    override open func viewDidLoad() {
        super.viewDidLoad()
        interactivePopGestureRecognizer?.delegate = self
    }

    /// Not on the first screen, where there is nothing to go back to.
    public func gestureRecognizerShouldBegin(_ gestureRecognizer: UIGestureRecognizer) -> Bool {
        viewControllers.count > 1
    }
}

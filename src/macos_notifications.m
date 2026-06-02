#import <Cocoa/Cocoa.h>
#import <Foundation/Foundation.h>

@interface PullbellNotificationDelegate : NSObject <NSUserNotificationCenterDelegate>
@end

@implementation PullbellNotificationDelegate

- (BOOL)userNotificationCenter:(NSUserNotificationCenter*)center
     shouldPresentNotification:(NSUserNotification*)notification {
    return YES;
}

- (void)userNotificationCenter:(NSUserNotificationCenter*)center
       didActivateNotification:(NSUserNotification*)notification {
    NSString* urlString = notification.userInfo[@"url"];
    if (urlString.length > 0) {
        NSURL* url = [NSURL URLWithString:urlString];
        if (url != nil) {
            [[NSWorkspace sharedWorkspace] openURL:url];
        }
    }

    [center removeDeliveredNotification:notification];
}

@end

static PullbellNotificationDelegate* pullbellNotificationDelegate = nil;

void pullbell_install_notification_delegate(void) {
    @autoreleasepool {
        if (pullbellNotificationDelegate == nil) {
            pullbellNotificationDelegate = [[PullbellNotificationDelegate alloc] init];
        }

        [NSUserNotificationCenter defaultUserNotificationCenter].delegate =
            pullbellNotificationDelegate;
    }
}

bool pullbell_send_pr_notification(
    const char* title,
    const char* subtitle,
    const char* message,
    const char* url
) {
    @autoreleasepool {
        pullbell_install_notification_delegate();

        NSString* titleString = [NSString stringWithUTF8String:title];
        NSString* subtitleString = [NSString stringWithUTF8String:subtitle];
        NSString* messageString = [NSString stringWithUTF8String:message];
        NSString* urlString = [NSString stringWithUTF8String:url];

        if (titleString == nil || messageString == nil || urlString == nil) {
            return false;
        }

        NSUserNotification* notification = [[NSUserNotification alloc] init];
        notification.title = titleString;
        if (subtitleString.length > 0) {
            notification.subtitle = subtitleString;
        }
        notification.informativeText = messageString;
        notification.userInfo = @{@"url": urlString};
        notification.hasActionButton = NO;

        [[NSUserNotificationCenter defaultUserNotificationCenter]
            deliverNotification:notification];

        return true;
    }
}
